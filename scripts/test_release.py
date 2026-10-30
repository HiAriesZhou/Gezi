"""Exercise release decisions and HTTP transport without publishing anything."""

import contextlib
from copy import deepcopy
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError

sys.dont_write_bytecode = True
import release

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

    def request(self, path, payload=None, missing_ok=False):
        self.calls.append((path, deepcopy(payload)))
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
            return {"object": {"type": "tag" if self.annotation else "commit", "sha": self.tag}}
        if path.startswith("git/tags/"):
            return {"object": {"type": "commit", "sha": self.annotation}}
        if path == "git/ref/heads/main":
            return {"object": {"type": "commit", "sha": self.sha}}
        if path == "releases" and payload is not None:
            if not self.fail_post or self.write_before_failure:
                if self.lose_tag:
                    self.tag = None
                elif self.tag is None:
                    self.tag = payload["target_commitish"]
                self.release = dict(payload, id=1)
            if self.fail_post:
                raise release.ReleaseError(self.fail_post)
            return deepcopy(self.release)
        raise AssertionError((path, payload))

    def posts(self):
        return [payload for _, payload in self.calls if payload is not None]


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

    def test_creates_once_at_checked_commit_with_current_notes(self):
        result = release.publish(self.root, self.api, SHA)
        self.assertEqual(result, f"Published v0.0.1 at {SHA}")
        self.assertEqual(self.api.posts(), [{
            "tag_name": "v0.0.1", "target_commitish": SHA, "name": "tmd v0.0.1",
            "body": "中文 release notes.", "draft": False, "prerelease": False,
        }])
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

    def test_post_failure_without_release_is_not_blindly_retried(self):
        self.api.fail_post = "HTTP 403"
        with self.assertRaisesRegex(release.ReleaseError, "HTTP 403"):
            release.publish(self.root, self.api, SHA)
        self.assertEqual(len(self.api.posts()), 1)

    def test_successful_response_without_verified_tag_fails(self):
        self.api.lose_tag = True
        with self.assertRaisesRegex(release.ReleaseError, "Could not verify"):
            release.publish(self.root, self.api, SHA)

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
    def test_post_method_json_headers_and_timeout(self):
        response = io.BytesIO(b'{"id":1}')
        with patch.object(release, "urlopen", return_value=response) as opened:
            api = release.GitHub("HiAriesZhou/tmd", "test-placeholder")
            self.assertEqual(api.request("releases", {"tag_name": "v0.0.1"}), {"id": 1})
            request = opened.call_args.args[0]
            self.assertEqual(request.get_method(), "POST")
            self.assertEqual(json.loads(request.data), {"tag_name": "v0.0.1"})
            self.assertEqual(request.get_header("Authorization"), "Bearer test-placeholder")
            self.assertEqual(opened.call_args.kwargs["timeout"], 30)

    def test_errors_are_redacted_and_only_expected_404_is_ignored(self):
        api = release.GitHub("HiAriesZhou/tmd", "test-placeholder")
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
