#!/usr/bin/env python3
"""Publish the checked main commit once, using only Python's standard library."""

import json
import hashlib
import os
from pathlib import Path
import sys
import uuid
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

from package_release import MAX_BYTES, verify_package
from release_metadata import REPOSITORY, SHA, ReleaseError, version_and_notes

ASSET_DIRECTORY = Path("target/release-assets")


class AssetRedirects(HTTPRedirectHandler):
    def __init__(self, allowed):
        self.allowed = allowed

    def redirect_request(self, request, fp, code, msg, headers, newurl):
        target = urlsplit(newurl)
        if (not self.allowed or request.get_method() != "GET" or target.scheme != "https"
                or target.hostname not in {"release-assets.githubusercontent.com",
                                           "github-releases.githubusercontent.com",
                                           "objects.githubusercontent.com"}
                or target.username or target.password or target.port not in (None, 443)):
            raise URLError("Unexpected GitHub redirect")
        redirected = super().redirect_request(request, fp, code, msg, headers, newurl)
        redirected.remove_header("Authorization")
        return redirected


def urlopen(request, timeout=30, allow_asset_redirects=False):
    return build_opener(AssetRedirects(allow_asset_redirects)).open(request, timeout=timeout)


def positive_id(value):
    if type(value) is not int or value <= 0:
        raise ReleaseError("GitHub returned an invalid object ID")
    return value


class GitHub:
    def __init__(self, repository, token):
        if not REPOSITORY.fullmatch(repository):
            raise ReleaseError("Invalid GitHub repository")
        self.base = f"https://api.github.com/repos/{repository}/"
        self.upload_base = f"https://uploads.github.com/repos/{repository}/"
        self.token = token

    def headers(self, content_type="application/json", accept="application/vnd.github+json"):
        return {"Accept": accept, "Authorization": f"Bearer {self.token}",
                "Content-Type": content_type, "X-GitHub-Api-Version": "2026-03-10"}

    def request(self, path, payload=None, missing_ok=False, method=None):
        request = Request(
            self.base + path,
            data=json.dumps(payload).encode() if payload is not None else None,
            headers=self.headers(), method=method,
        )
        return self.json_response(request, path, missing_ok)

    def json_response(self, request, operation, missing_ok=False):
        try:
            with urlopen(request, timeout=30) as response:
                return json.load(response)
        except HTTPError as error:
            if missing_ok and error.code == 404:
                return None
            # Do not print request headers, tokens, or server response bodies.
            raise ReleaseError(f"GitHub API {request.get_method()} {operation}: HTTP {error.code}") from None
        except (URLError, TimeoutError, OSError, ValueError):
            raise ReleaseError(f"GitHub API {request.get_method()} {operation}: response unavailable") from None

    def upload_asset(self, release_id, asset):
        release_id = positive_id(release_id)
        with asset.path.open("rb") as source:
            payload = source.read(MAX_BYTES + 1)
        if len(payload) != asset.size or hashlib.sha256(payload).hexdigest() != asset.sha256:
            raise ReleaseError("Local asset changed before upload")
        path = f"releases/{release_id}/assets?name=" + quote(asset.name, safe="")
        request = Request(self.upload_base + path, data=payload,
                          headers=self.headers("application/octet-stream"), method="POST")
        return self.json_response(request, f"release {release_id} asset upload")

    def download_asset(self, asset_id):
        asset_id = positive_id(asset_id)
        request = Request(self.base + f"releases/assets/{asset_id}",
                          headers=self.headers(accept="application/octet-stream"))
        try:
            with urlopen(request, timeout=30, allow_asset_redirects=True) as response:
                data = response.read(MAX_BYTES + 1)
            if len(data) > MAX_BYTES:
                raise ReleaseError("Downloaded asset exceeds the size limit")
            return data
        except HTTPError as error:
            raise ReleaseError(f"GitHub asset {asset_id} download: HTTP {error.code}") from None
        except (URLError, TimeoutError, OSError, ValueError):
            raise ReleaseError(f"GitHub asset {asset_id} download: response unavailable") from None


def tag_commit(api, tag):
    ref = api.request("git/ref/tags/" + quote(tag, safe=""), missing_ok=True)
    if ref is None:
        return None
    obj = ref["object"]
    # Accept both lightweight tags and existing annotated tags, never move either.
    for _ in range(8):
        if obj["type"] == "commit" and SHA.fullmatch(obj["sha"]):
            return obj["sha"]
        if obj["type"] != "tag" or not SHA.fullmatch(obj["sha"]):
            break
        obj = api.request("git/tags/" + obj["sha"])["object"]
    raise ReleaseError(f"Cannot resolve {tag} to a commit")


def draft_target_matches(api, tag, sha, draft):
    # GitHub ignores target_commitish when the tag already exists. The tag's
    # resolved commit is authoritative, including immediately before publish.
    target = tag_commit(api, tag)
    return target == sha if target is not None else draft.get("target_commitish") == sha


def release_assets(api, release_id):
    result = []
    page = 1
    while True:
        items = api.request(f"releases/{positive_id(release_id)}/assets?per_page=100&page={page}")
        result.extend(items)
        if len(items) < 100:
            return result
        page += 1


def verify_assets(api, release_id, expected):
    assets = release_assets(api, release_id)
    if len(assets) != len(expected) or {item.get("name") for item in assets} != {a.name for a in expected}:
        raise ReleaseError("Release asset inventory does not match the package")
    for asset in expected:
        matches = [item for item in assets if item.get("name") == asset.name]
        if len(matches) != 1:
            raise ReleaseError("Release has duplicate asset names")
        verify_asset(api, matches[0], asset)


def verify_asset(api, remote, expected):
    if (remote.get("state") != "uploaded" or remote.get("size") != expected.size
            or remote.get("digest") not in (None, "sha256:" + expected.sha256)):
        raise ReleaseError("Uploaded asset metadata does not match the package")
    data = api.download_asset(positive_id(remote.get("id")))
    if len(data) != expected.size or hashlib.sha256(data).hexdigest() != expected.sha256:
        raise ReleaseError("Downloaded asset content does not match the package")


def verified_release(api, tag, sha, release, expected_id, assets, notes):
    if (release is None or release.get("draft") is not False
            or release.get("tag_name") != tag or release.get("id") != expected_id
            or release.get("body") != notes or release.get("name") != f"tmd {tag}"
            or release.get("prerelease") is not ("-" in tag.partition("+")[0])
            or tag_commit(api, tag) != sha):
        raise ReleaseError(f"Could not verify published {tag} at the checked commit; inspect GitHub before rerunning")
    verify_assets(api, expected_id, assets)
    return f"Published {tag} at {sha}"


def existing_release(api, tag):
    published = api.request("releases/tags/" + quote(tag, safe=""), missing_ok=True)
    if published is not None:
        return published
    # Get-by-tag excludes drafts; list explicitly so an author's draft is not published.
    page = 1
    while True:
        releases = api.request(f"releases?per_page=100&page={page}")
        for release in releases:
            if release.get("tag_name") == tag:
                return release
        if len(releases) < 100:
            return None
        page += 1


def publish(root, api, sha):
    if not SHA.fullmatch(sha):
        raise ReleaseError("Expected an exact checked commit SHA")
    version, notes = version_and_notes(root)
    tag = "v" + version
    release_path = "releases/tags/" + quote(tag, safe="")
    existing = existing_release(api, tag)
    target = tag_commit(api, tag)
    if existing is not None:
        if existing.get("draft") is not False or target is None:
            raise ReleaseError(f"{tag} already has an incomplete release; inspect it manually")
        return f"Skipped {tag}: already published"
    if target is not None and target != sha:
        raise ReleaseError(f"{tag} points to another commit without a published release; refusing to move it")
    # Check immediately before writing. Always publish the CI SHA, never a branch name.
    current = api.request("git/ref/heads/main")["object"]["sha"]
    if current != sha:
        return "Skipped release: main has advanced beyond this CI run"
    assets = verify_package(root / ASSET_DIRECTORY, version, sha)
    marker = "<!-- tmd-release-owner:" + uuid.uuid4().hex + " -->"
    draft_body = notes + "\n\n" + marker
    payload = {
        "tag_name": tag,
        "target_commitish": sha,
        "name": f"tmd {tag}",
        "body": draft_body,
        "draft": True,
        "prerelease": "-" in version.partition("+")[0],
    }
    try:
        created = api.request("releases", payload)
    except ReleaseError as error:
        # A failed response may hide a successful write. Read back, never POST twice.
        created = existing_release(api, tag)
        if (created is None or created.get("draft") is not True
                or created.get("body") != draft_body
                or not draft_target_matches(api, tag, sha, created)):
            raise error
    release_id = positive_id(created.get("id"))

    def owned_draft():
        draft = api.request(f"releases/{release_id}")
        if (positive_id(draft.get("id")) != release_id or draft.get("tag_name") != tag
                or draft.get("draft") is not True or draft.get("body") != draft_body
                or not draft_target_matches(api, tag, sha, draft) or draft.get("name") != payload["name"]
                or draft.get("prerelease") is not payload["prerelease"]):
            raise ReleaseError("Draft ownership or contents changed; refusing to publish")

    owned_draft()
    if release_assets(api, release_id):
        raise ReleaseError("New draft unexpectedly already contains assets")
    for asset in assets:
        try:
            api.upload_asset(release_id, asset)
        except ReleaseError as error:
            found = [a for a in release_assets(api, release_id) if a.get("name") == asset.name]
            if len(found) != 1:
                raise error
            verify_asset(api, found[0], asset)
    verify_assets(api, release_id, assets)
    owned_draft()
    if api.request("git/ref/heads/main")["object"]["sha"] != sha:
        raise ReleaseError("Main advanced; owned draft remains unpublished for inspection")
    try:
        api.request(f"releases/{release_id}", {"draft": False, "body": notes}, method="PATCH")
    except ReleaseError as error:
        recovered = api.request(release_path, missing_ok=True)
        if recovered is None:
            raise error
        return verified_release(api, tag, sha, recovered, release_id, assets, notes)
    created = api.request(release_path, missing_ok=True)
    return verified_release(api, tag, sha, created, release_id, assets, notes)


def main():
    if os.environ.get("GITHUB_EVENT_NAME") != "push" or os.environ.get("GITHUB_REF") != "refs/heads/main":
        print("Skipped release: only main push events may publish")
        return 0
    try:
        token = os.environ.get("GITHUB_TOKEN")
        if not token:
            raise ReleaseError("GITHUB_TOKEN is required")
        api = GitHub(os.environ.get("GITHUB_REPOSITORY", ""), token)
        print(publish(Path(__file__).resolve().parents[1], api, os.environ.get("GITHUB_SHA", "")))
        return 0
    except (ReleaseError, KeyError, OSError, ValueError) as error:
        # Configuration errors contain no request headers or credential values.
        print(f"Release failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
