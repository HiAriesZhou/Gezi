#!/usr/bin/env python3
"""Publish the checked main commit once, using only Python's standard library."""

import json
import os
from pathlib import Path
import re
import sys
import tomllib
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen


IDENTIFIER = r"(?:0|[1-9][0-9]*)"
VERSION = re.compile(
    rf"{IDENTIFIER}\.{IDENTIFIER}\.{IDENTIFIER}"
    r"(?:-(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)
SHA = re.compile(r"[0-9a-f]{40}")
REPOSITORY = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+")


class ReleaseError(Exception):
    pass


class GitHub:
    def __init__(self, repository, token):
        if not REPOSITORY.fullmatch(repository):
            raise ReleaseError("Invalid GitHub repository")
        self.base = f"https://api.github.com/repos/{repository}/"
        self.token = token

    def request(self, path, payload=None, missing_ok=False):
        request = Request(
            self.base + path,
            data=json.dumps(payload).encode() if payload is not None else None,
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
                "X-GitHub-Api-Version": "2026-03-10",
            },
        )
        try:
            with urlopen(request, timeout=30) as response:
                return json.load(response)
        except HTTPError as error:
            if missing_ok and error.code == 404:
                return None
            # Do not print request headers, tokens, or server response bodies.
            raise ReleaseError(f"GitHub API {request.get_method()} {path}: HTTP {error.code}") from None
        except (URLError, TimeoutError, OSError, ValueError):
            raise ReleaseError(f"GitHub API {request.get_method()} {path}: response unavailable") from None


def version_and_notes(root):
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    version = manifest["workspace"]["package"]["version"]
    if not isinstance(version, str) or not VERSION.fullmatch(version):
        raise ReleaseError("Cargo workspace version must be exact SemVer")
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    for member in manifest["workspace"]["members"]:
        package = tomllib.loads((root / member / "Cargo.toml").read_text())["package"]
        declared = package["version"]
        if declared != {"workspace": True} and declared != version:
            raise ReleaseError(f"Version mismatch in {member}")
        locked = [p for p in lock["package"] if p["name"] == package["name"]]
        if len(locked) != 1 or locked[0]["version"] != version:
            raise ReleaseError(f"Version mismatch in Cargo.lock for {package['name']}")
    changelog = (root / "CHANGELOG.md").read_text()
    sections = re.split(r"^## ", changelog, flags=re.MULTILINE)[1:]
    matches = [s.partition("\n")[2].strip() for s in sections if s.partition("\n")[0].strip() == version]
    if len(matches) != 1 or not matches[0]:
        raise ReleaseError(f"CHANGELOG.md needs one nonempty '## {version}' section")
    return version, matches[0]


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


def verified_release(api, tag, sha, release):
    if (release is None or release.get("draft") is not False
            or release.get("tag_name") != tag or not release.get("id")
            or tag_commit(api, tag) != sha):
        raise ReleaseError(f"Could not verify published {tag} at the checked commit; inspect GitHub before rerunning")
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
    payload = {
        "tag_name": tag,
        "target_commitish": sha,
        "name": f"tmd {tag}",
        "body": notes,
        "draft": False,
        "prerelease": "-" in version.partition("+")[0],
    }
    try:
        # GitHub creates the tag at target_commitish when it does not exist.
        api.request("releases", payload)
    except ReleaseError as error:
        # A failed response may hide a successful write. Read back, never POST twice.
        recovered = api.request(release_path, missing_ok=True)
        if recovered is None:
            raise error
        return verified_release(api, tag, sha, recovered)
    created = api.request(release_path, missing_ok=True)
    return verified_release(api, tag, sha, created)


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
