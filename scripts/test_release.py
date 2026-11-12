"""Exercise release decisions and HTTP transport without publishing anything."""

import contextlib
from copy import deepcopy
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import shutil
import subprocess
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError

sys.dont_write_bytecode = True
import release
import package_release as package

SHA = "a" * 40
OTHER = "b" * 40


class FakeGitHub:
    def __init__(self):
        self.sha = SHA
        self.tag = None
        self.release = None
        self.calls = []
        self.fail_post = None
        self.write_before_failure = False
        self.lose_tag = False
        self.annotation = None
        self.draft_on_page_two = False
        self.assets = {}
        self.asset_data = {}
        self.upload_count = 0
        self.fail_upload_at = None
        self.write_upload_before_failure = False
        self.fail_patch = None
        self.write_patch_before_failure = False
        self.corrupt_download = False
        self.bad_digest = False
        self.change_owner = False
        self.advance_after_upload = False
        self.bad_inventory = None
        self.changed_draft_field = None
        self.normalize_target_field = False
        self.move_tag_after_upload = False

    def request(self, path, payload=None, missing_ok=False, method=None):
        method = method or ("POST" if payload is not None else "GET")
        self.calls.append((path, deepcopy(payload), method))
        if path.startswith("releases/tags/"):
            if self.release and not self.release["draft"]:
                return deepcopy(self.release)
            return None
        if path.startswith("releases?per_page="):
            if self.draft_on_page_two and path.endswith("page=1"):
                return [{"tag_name": "other"}] * 100
            return [deepcopy(self.release)] if self.release else []
        if path.startswith("git/ref/tags/"):
            if self.tag is None:
                return None
            target = OTHER if self.move_tag_after_upload and len(self.assets) == 2 else self.tag
            return {"object": {"type": "tag" if self.annotation else "commit", "sha": target}}
        if path.startswith("git/tags/"):
            return {"object": {"type": "commit", "sha": self.annotation}}
        if path == "git/ref/heads/main":
            sha = OTHER if self.advance_after_upload and self.assets else self.sha
            return {"object": {"type": "commit", "sha": sha}}
        if path.startswith("releases/1/assets?"):
            items = list(deepcopy(self.assets).values())
            if len(items) == 2:
                if self.bad_inventory == "extra":
                    items.append({"id": 99, "name": "foreign"})
                elif self.bad_inventory == "duplicate":
                    items[1]["name"] = items[0]["name"]
                elif self.bad_inventory == "starter":
                    items[0]["state"] = "starter"
            return items
        if path == "releases/1" and method == "GET":
            draft = deepcopy(self.release)
            if self.assets and self.changed_draft_field:
                field, value = self.changed_draft_field
                draft[field] = value
            return draft
        if path == "releases/1" and method == "PATCH":
            if not self.fail_patch or self.write_patch_before_failure:
                self.release.update(payload)
                if self.lose_tag:
                    self.tag = None
            if self.fail_patch:
                raise release.ReleaseError(self.fail_patch)
            return deepcopy(self.release)
        if path == "releases" and method == "POST":
            if not self.fail_post or self.write_before_failure:
                if self.lose_tag:
                    self.tag = None
                elif self.tag is None:
                    self.tag = payload["target_commitish"]
                self.release = dict(payload, id=1)
                self.assets = {}
                self.asset_data = {}
                if self.change_owner:
                    self.release["body"] = "another draft owner"
                if self.normalize_target_field:
                    self.release["target_commitish"] = "main"
            if self.fail_post:
                raise release.ReleaseError(self.fail_post)
            return deepcopy(self.release)
        raise AssertionError((path, payload))

    def posts(self):
        return [payload for path, payload, method in self.calls if path == "releases" and method == "POST"]

    def patches(self):
        return [payload for _, payload, method in self.calls if method == "PATCH"]

    def upload_asset(self, release_id, asset):
        self.upload_count += 1
        self.calls.append((f"upload/{asset.name}", None, "UPLOAD"))
        failed = self.upload_count == self.fail_upload_at
        if not failed or self.write_upload_before_failure:
            asset_id = len(self.assets) + 10
            self.assets[asset.name] = {"id": asset_id, "name": asset.name, "size": asset.size,
                                       "state": "uploaded", "digest": "sha256:" + asset.sha256}
            if self.bad_digest:
                self.assets[asset.name]["digest"] = "sha256:wrong"
            self.asset_data[asset_id] = asset.path.read_bytes()
        if failed:
            raise release.ReleaseError("upload response unavailable")

    def download_asset(self, asset_id):
        self.calls.append((f"download/{asset_id}", None, "DOWNLOAD"))
        return self.asset_data[asset_id] + (b"corrupted" if self.corrupt_download else b"")


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="tmd-release-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.configure("0.0.1")
        self.api = FakeGitHub()

    def configure(self, version):
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["cli"]\n[workspace.package]\nversion = ' + json.dumps(version) + '\n')
        (self.root / "cli").mkdir(exist_ok=True)
        (self.root / "cli/Cargo.toml").write_text('[package]\nname = "tmd-cli"\nversion.workspace = true\n')
        (self.root / "Cargo.lock").write_text('[[package]]\nname = "tmd-cli"\nversion = ' + json.dumps(version) + '\n')
        (self.root / "CHANGELOG.md").write_text(f'# 版本记录\n\n## {version}\n\n中文 release notes.\n\n## 0.0.0\n\nOld.\n')
        if package.VERSION.fullmatch(version):
            assets = self.root / release.ASSET_DIRECTORY
            if assets.exists():
                shutil.rmtree(assets)
            (self.root / "docs").mkdir(exist_ok=True)
            (self.root / "LICENSE").write_text("MIT License\n")
            (self.root / "docs/binary-package.md").write_text("Run ./gezi --version\n")
            binary = self.root / "gezi"
            binary.write_bytes(bytes.fromhex("cffaedfe") + (0x0100000c).to_bytes(4, "little")
                               + bytes(4) + (2).to_bytes(4, "little") + bytes(16))
            binary.chmod(0o755)
            with patch.object(package.subprocess, "run", return_value=
                              subprocess.CompletedProcess([], 0, f"gezi {version}\n".encode(), b"")):
                package.create_package(self.root, binary, assets, SHA)

    def test_creates_once_at_checked_commit_with_current_notes(self):
        result = release.publish(self.root, self.api, SHA)
        self.assertEqual(result, f"Published v0.0.1 at {SHA}")
        created = self.api.posts()[0]
        self.assertEqual(created["tag_name"], "v0.0.1")
        self.assertEqual(created["target_commitish"], SHA)
        self.assertTrue(created["draft"])
        self.assertTrue(created["body"].startswith("中文 release notes.\n\n<!-- gezi-release-owner:"))
        self.assertEqual(self.api.patches(), [{"draft": False, "body": "中文 release notes."}])
        self.assertEqual(len(self.api.assets), 2)
        self.assertFalse(self.api.release["draft"])
        self.assertEqual(self.api.release["body"], "中文 release notes.")
        result = release.publish(self.root, self.api, SHA)
        self.assertIn("already published", result)
        self.assertEqual(len(self.api.posts()), 1)

    def test_same_published_version_on_later_commit_is_skipped(self):
        release.publish(self.root, self.api, SHA)
        self.api.sha = OTHER
        self.assertIn("already published", release.publish(self.root, self.api, OTHER))
        self.assertEqual(self.api.tag, SHA)
        self.assertEqual(len(self.api.posts()), 1)

    def test_stale_run_cannot_publish(self):
        self.api.sha = OTHER
        self.assertIn("main has advanced", release.publish(self.root, self.api, SHA))
        self.assertFalse(self.api.posts())

    def test_existing_tag_can_resume_only_at_same_checked_commit(self):
        self.api.tag = SHA
        self.assertIn("Published", release.publish(self.root, self.api, SHA))
        self.api.release = None
        self.api.tag = OTHER
        before = len(self.api.posts())
        with self.assertRaisesRegex(release.ReleaseError, "refusing to move"):
            release.publish(self.root, self.api, SHA)
        self.assertEqual(len(self.api.posts()), before)
        self.assertEqual(self.api.tag, OTHER)

    def test_existing_annotated_tag_resolves_without_retagging(self):
        self.api.tag = OTHER
        self.api.annotation = SHA
        self.assertIn("Published", release.publish(self.root, self.api, SHA))
        # The request must reference the checked commit even with an existing tag.
        self.assertEqual(self.api.posts()[0]["target_commitish"], SHA)
        self.assertEqual(self.api.tag, OTHER)
        self.assertEqual(self.api.annotation, SHA)

    def test_existing_draft_is_not_published_even_on_later_page(self):
        self.api.release = {"id": 5, "tag_name": "v0.0.1", "draft": True}
        self.api.draft_on_page_two = True
        with self.assertRaisesRegex(release.ReleaseError, "incomplete release"):
            release.publish(self.root, self.api, SHA)
        self.assertFalse(self.api.posts())
        self.assertTrue(self.api.release["draft"])

    def test_post_response_lost_after_success_reads_back_without_retry(self):
        self.api.fail_post = "response unavailable"
        self.api.write_before_failure = True
        self.assertIn("Published", release.publish(self.root, self.api, SHA))
        self.assertEqual(len(self.api.posts()), 1)
        self.assertEqual(len(self.api.patches()), 1)

    def test_post_failure_without_release_is_not_blindly_retried(self):
        self.api.fail_post = "HTTP 403"
        with self.assertRaisesRegex(release.ReleaseError, "HTTP 403"):
            release.publish(self.root, self.api, SHA)
        self.assertEqual(len(self.api.posts()), 1)

    def test_successful_response_without_verified_tag_fails(self):
        self.api.lose_tag = True
        with self.assertRaisesRegex(release.ReleaseError, "Could not verify"):
            release.publish(self.root, self.api, SHA)

    def test_partial_upload_leaves_owned_draft_unpublished(self):
        self.api.fail_upload_at = 2
        with self.assertRaisesRegex(release.ReleaseError, "upload"):
            release.publish(self.root, self.api, SHA)
        self.assertTrue(self.api.release["draft"])
        self.assertEqual(len(self.api.assets), 1)
        self.assertFalse(self.api.patches())

    def test_upload_response_lost_reads_back_and_never_uploads_twice(self):
        self.api.fail_upload_at = 1
        self.api.write_upload_before_failure = True
        self.assertIn("Published", release.publish(self.root, self.api, SHA))
        self.assertEqual(self.api.upload_count, 2)
        self.assertEqual(len(self.api.patches()), 1)

    def test_corrupt_download_or_digest_prevents_publication(self):
        for attribute in ("corrupt_download", "bad_digest"):
            with self.subTest(attribute=attribute):
                self.api = FakeGitHub()
                setattr(self.api, attribute, True)
                with self.assertRaisesRegex(release.ReleaseError, "asset"):
                    release.publish(self.root, self.api, SHA)
                self.assertTrue(self.api.release["draft"])
                self.assertFalse(self.api.patches())

    def test_changed_ownership_after_uncertain_create_is_not_adopted(self):
        self.api.fail_post = "create response unavailable"
        self.api.write_before_failure = True
        self.api.change_owner = True
        with self.assertRaisesRegex(release.ReleaseError, "create response"):
            release.publish(self.root, self.api, SHA)
        self.assertEqual(len(self.api.posts()), 1)
        self.assertEqual(self.api.upload_count, 0)
        self.assertFalse(self.api.patches())

    def test_existing_failed_draft_is_never_resumed_by_another_run(self):
        self.api.fail_upload_at = 2
        with self.assertRaises(release.ReleaseError):
            release.publish(self.root, self.api, SHA)
        before = len(self.api.calls)
        with self.assertRaisesRegex(release.ReleaseError, "incomplete release"):
            release.publish(self.root, self.api, SHA)
        self.assertEqual(len(self.api.posts()), 1)
        self.assertEqual(self.api.upload_count, 2)
        self.assertFalse(self.api.patches())
        self.assertGreater(len(self.api.calls), before)

    def test_publish_response_lost_after_write_is_verified_without_retry(self):
        self.api.fail_patch = "publish response unavailable"
        self.api.write_patch_before_failure = True
        self.assertIn("Published", release.publish(self.root, self.api, SHA))
        self.assertEqual(len(self.api.patches()), 1)

    def test_publish_failure_without_write_leaves_draft_unpublished(self):
        self.api.fail_patch = "publish response unavailable"
        with self.assertRaisesRegex(release.ReleaseError, "publish response"):
            release.publish(self.root, self.api, SHA)
        self.assertTrue(self.api.release["draft"])
        self.assertEqual(len(self.api.patches()), 1)

    def test_main_advances_after_upload_cannot_publish_old_commit(self):
        self.api.advance_after_upload = True
        with self.assertRaisesRegex(release.ReleaseError, "Main advanced"):
            release.publish(self.root, self.api, SHA)
        self.assertTrue(self.api.release["draft"])
        self.assertEqual(len(self.api.assets), 2)
        self.assertFalse(self.api.patches())

    def test_missing_or_modified_local_package_prevents_any_write(self):
        next((self.root / release.ASSET_DIRECTORY).glob("*.sha256")).write_text("wrong checksum\n")
        with self.assertRaisesRegex(release.ReleaseError, "checksum"):
            release.publish(self.root, self.api, SHA)
        self.assertFalse(self.api.posts())

    def test_extra_duplicate_or_unfinished_remote_assets_never_publish(self):
        for inventory in ("extra", "duplicate", "starter"):
            with self.subTest(inventory=inventory):
                self.api = FakeGitHub()
                self.api.bad_inventory = inventory
                with self.assertRaisesRegex(release.ReleaseError, "asset"):
                    release.publish(self.root, self.api, SHA)
                self.assertTrue(self.api.release["draft"])
                self.assertFalse(self.api.patches())

    def test_draft_edits_after_upload_are_preserved_without_publishing(self):
        for field, value in [("body", "maintainer edit"), ("name", "renamed"), ("prerelease", True)]:
            with self.subTest(field=field):
                self.api = FakeGitHub()
                self.api.changed_draft_field = (field, value)
                with self.assertRaisesRegex(release.ReleaseError, "Draft ownership or contents"):
                    release.publish(self.root, self.api, SHA)
                self.assertEqual(len(self.api.assets), 2)
                self.assertFalse(self.api.patches())

    def test_existing_tag_is_authoritative_when_commitish_field_is_unused(self):
        for response_lost in (False, True):
            with self.subTest(response_lost=response_lost):
                self.api = FakeGitHub()
                self.api.tag = SHA
                self.api.normalize_target_field = True
                if response_lost:
                    self.api.fail_post = "response unavailable"
                    self.api.write_before_failure = True
                self.assertIn("Published", release.publish(self.root, self.api, SHA))
                self.assertEqual(self.api.tag, SHA)
                self.assertEqual(len(self.api.posts()), 1)

    def test_tag_changed_after_upload_is_detected_before_publication(self):
        self.api.move_tag_after_upload = True
        with self.assertRaises(release.ReleaseError):
            release.publish(self.root, self.api, SHA)
        self.assertFalse(self.api.patches())
        self.assertTrue(self.api.release["draft"])

    def test_prerelease_is_marked_and_build_metadata_is_supported(self):
        self.configure("0.1.0-rc.1+build.1")
        release.publish(self.root, self.api, SHA)
        self.assertTrue(self.api.posts()[0]["prerelease"])
        self.assertEqual(self.api.posts()[0]["tag_name"], "v0.1.0-rc.1+build.1")

    def test_invalid_versions_and_lock_mismatch_fail_before_network(self):
        for version in ["v0.0.1", "0.01.0", "0.0.1-01", "0.0.1\n", "0.0.1;echo bad"]:
            with self.subTest(version=version):
                self.configure(version)
                with self.assertRaises(release.ReleaseError):
                    release.publish(self.root, self.api, SHA)
        self.configure("0.0.1")
        (self.root / "Cargo.lock").write_text('[[package]]\nname="tmd-cli"\nversion="0.0.2"\n')
        with self.assertRaisesRegex(release.ReleaseError, "Version mismatch"):
            release.publish(self.root, self.api, SHA)
        self.assertFalse(self.api.calls)

    def test_missing_empty_or_duplicate_notes_fail_before_network(self):
        for text in ["## 0.0.2\nOther", "## 0.0.1\n\n", "## 0.0.1\nA\n## 0.0.1\nB"]:
            with self.subTest(text=text):
                (self.root / "CHANGELOG.md").write_text(text)
                with self.assertRaisesRegex(release.ReleaseError, "CHANGELOG"):
                    release.publish(self.root, self.api, SHA)
        self.assertFalse(self.api.calls)

    def test_non_main_events_skip_without_constructing_client(self):
        for event, ref in [("pull_request", "refs/pull/14/merge"), ("push", "refs/heads/feature"),
                           ("push", "refs/tags/v0.0.1"), ("workflow_dispatch", "refs/heads/main")]:
            with self.subTest(event=event), patch.dict(os.environ, {"GITHUB_EVENT_NAME": event, "GITHUB_REF": ref}, clear=True):
                with patch.object(release, "GitHub") as client, contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(release.main(), 0)
                    client.assert_not_called()

    def test_missing_token_and_invalid_sha_are_refused(self):
        with patch.dict(os.environ, {"GITHUB_EVENT_NAME": "push", "GITHUB_REF": "refs/heads/main"}, clear=True):
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(release.main(), 1)
        with self.assertRaisesRegex(release.ReleaseError, "commit SHA"):
            release.publish(self.root, self.api, "main")
        self.assertFalse(self.api.calls)


class TransportTests(unittest.TestCase):
    def test_only_signed_asset_hosts_accept_get_redirects_without_credentials(self):
        request = release.Request("https://api.github.com/repos/owner/repo/releases/assets/1",
                                  headers={"Authorization": "Bearer test-placeholder"})
        redirected = release.AssetRedirects(True).redirect_request(
            request, None, 302, "Found", {}, "https://release-assets.githubusercontent.com/asset?signature=test")
        self.assertIsNone(redirected.get_header("Authorization"))
        for allowed, url in [(False, "https://release-assets.githubusercontent.com/asset"),
                             (True, "https://example.test/asset"),
                             (True, "http://release-assets.githubusercontent.com/asset"),
                             (True, "https://user@release-assets.githubusercontent.com/asset")]:
            with self.subTest(allowed=allowed, url=url), self.assertRaises(URLError):
                release.AssetRedirects(allowed).redirect_request(request, None, 302, "Found", {}, url)
        upload = release.Request(request.full_url, data=b"payload", method="POST")
        with self.assertRaises(URLError):
            release.AssetRedirects(True).redirect_request(
                upload, None, 307, "Redirect", {}, "https://release-assets.githubusercontent.com/asset")

    def test_asset_upload_endpoint_and_download_limit(self):
        api = release.GitHub("owner/repo", "test-placeholder")
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "archive.tar.gz"
            path.write_bytes(b"asset payload")
            expected = package.asset(path)
            with patch.object(release, "urlopen", return_value=io.BytesIO(b'{"id":10}')) as opened:
                api.upload_asset(1, expected)
                request = opened.call_args.args[0]
                self.assertEqual(request.full_url,
                                 "https://uploads.github.com/repos/owner/repo/releases/1/assets?name=archive.tar.gz")
                self.assertEqual(request.data, b"asset payload")
                self.assertEqual(request.get_header("Authorization"), "Bearer test-placeholder")
            path.write_bytes(b"changed")
            with patch.object(release, "urlopen") as opened, self.assertRaisesRegex(release.ReleaseError, "changed"):
                api.upload_asset(1, expected)
            opened.assert_not_called()
        with patch.object(release, "urlopen", return_value=io.BytesIO(b"bytes")) as opened:
            self.assertEqual(api.download_asset(10), b"bytes")
            self.assertTrue(opened.call_args.kwargs["allow_asset_redirects"])
        with patch.object(release, "MAX_BYTES", 2), patch.object(release, "urlopen", return_value=io.BytesIO(b"long")):
            with self.assertRaisesRegex(release.ReleaseError, "size limit"):
                api.download_asset(10)
        for identifier in (True, "1/path", 0, -1):
            with self.subTest(identifier=identifier), self.assertRaises(release.ReleaseError):
                api.download_asset(identifier)

    def test_post_method_json_headers_and_timeout(self):
        response = io.BytesIO(b'{"id":1}')
        with patch.object(release, "urlopen", return_value=response) as opened:
            api = release.GitHub("HiAriesZhou/Gezi", "test-placeholder")
            self.assertEqual(api.request("releases", {"tag_name": "v0.0.1"}), {"id": 1})
            request = opened.call_args.args[0]
            self.assertEqual(request.get_method(), "POST")
            self.assertEqual(json.loads(request.data), {"tag_name": "v0.0.1"})
            self.assertEqual(request.get_header("Authorization"), "Bearer test-placeholder")
            self.assertEqual(opened.call_args.kwargs["timeout"], 30)

    def test_errors_are_redacted_and_only_expected_404_is_ignored(self):
        api = release.GitHub("HiAriesZhou/Gezi", "test-placeholder")
        error = HTTPError("https://api.github.com", 403, "secret-response", {}, io.BytesIO(b"secret-body"))
        with patch.object(release, "urlopen", side_effect=error):
            with self.assertRaises(release.ReleaseError) as caught:
                api.request("releases", missing_ok=True)
            self.assertEqual(str(caught.exception), "GitHub API GET releases: HTTP 403")
        error = HTTPError("https://api.github.com", 404, "missing", {}, None)
        with patch.object(release, "urlopen", side_effect=error):
            self.assertIsNone(api.request("releases", missing_ok=True))
            with self.assertRaises(release.ReleaseError):
                api.request("releases")
        for error in [URLError("secret-error"), TimeoutError("secret-error")]:
            with patch.object(release, "urlopen", side_effect=error):
                with self.assertRaisesRegex(release.ReleaseError, "response unavailable"):
                    api.request("releases")

    def test_repository_cannot_change_api_host_or_path(self):
        for repository in ["https://evil.test", "a/b/c", "a/b?token=x", "a/b\nheader"]:
            with self.assertRaises(release.ReleaseError):
                release.GitHub(repository, "test-placeholder")


if __name__ == "__main__":
    unittest.main()
