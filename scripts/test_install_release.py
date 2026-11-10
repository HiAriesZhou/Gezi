"""Exercise the shell installer without contacting GitHub or changing the host."""
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest


INSTALLER = Path(__file__).resolve().parents[1] / "install.sh"


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="tmd-install-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bin = self.root / "commands"
        self.bin.mkdir()
        self.destination = self.root / "personal bin"
        self.destination.mkdir()
        self.target = self.destination / "tmd"
        self.downloads = self.root / "downloads"
        self.downloads.mkdir()
        self.requests = self.root / "requests.jsonl"
        self.env = dict(os.environ, PATH=f"{self.bin}:/usr/bin:/bin",
                        TMD_INSTALL_DIR=str(self.destination), TMD_VERSION="v0.1.4",
                        TMD_TEST_ROOT=str(self.root), TMPDIR=str(self.root))
        self.command("uname", '#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; esac\n')
        self.command("curl", f"""#!{sys.executable}
import json, os, pathlib, sys
root = pathlib.Path(os.environ["TMD_TEST_ROOT"])
args = sys.argv[1:]
url = args[-1]
with (root / "requests.jsonl").open("a") as output:
    output.write(json.dumps(args) + "\\n")
if os.environ.get("TMD_TEST_DOWNLOAD_FAIL"):
    sys.exit(22)
if url.endswith("/latest"):
    print(url.removesuffix("/latest") + "/tag/v0.1.4", end="")
else:
    source = root / "downloads" / url.rsplit("/", 1)[-1]
    pathlib.Path(args[args.index("--output") + 1]).write_bytes(source.read_bytes())
""")
        self.package()

    def command(self, name, content):
        path = self.bin / name
        path.write_text(content)
        path.chmod(0o755)

    def package(self, binary=b"#!/bin/sh\necho 'tmd 0.1.4'\n", entry=None):
        name = "tmd-v0.1.4-aarch64-apple-darwin"
        archive = self.downloads / (name + ".tar.gz")
        with tarfile.open(archive, "w:gz") as package:
            info = tarfile.TarInfo(entry or (name + "/tmd"))
            info.size = len(binary)
            info.mode = 0o755
            package.addfile(info, io.BytesIO(binary))
        self.checksum = self.downloads / (archive.name + ".sha256")
        self.checksum.write_text(f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")
        return binary

    def run_installer(self, success=True, args=()):
        result = subprocess.run(["/bin/sh", str(INSTALLER), *args], env=self.env,
                                capture_output=True, text=True, timeout=15)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(list(self.destination.glob(".tmd-*")))
        self.assertFalse(list(self.root.glob("tmd-install.*")))
        return result

    def test_pinned_install_and_permissions(self):
        binary = self.package()
        self.run_installer()
        self.assertEqual(self.target.read_bytes(), binary)
        self.assertEqual(self.target.stat().st_mode & 0o777, 0o755)
        requests = [json.loads(line) for line in self.requests.read_text().splitlines()]
        self.assertEqual(len(requests), 2)
        for request in requests:
            self.assertIn("/download/v0.1.4/", request[-1])
            self.assertIn("--proto-redir", request)
            self.assertEqual(request[request.index("--proto-redir") + 1], "=https")

    def test_latest_resolves_once_then_pins_both_assets(self):
        del self.env["TMD_VERSION"]
        self.run_installer()
        requests = [json.loads(line)[-1] for line in self.requests.read_text().splitlines()]
        self.assertEqual(len(requests), 3)
        self.assertTrue(requests[0].endswith("/latest"))
        self.assertTrue(all("/download/v0.1.4/" in url for url in requests[1:]))

    def test_update_retains_old_binary_and_repeat_is_noop(self):
        self.target.write_bytes(b"old executable")
        self.target.chmod(0o755)
        self.run_installer()
        backups = list(self.destination.glob("tmd.backup.*"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(backups[0].read_bytes(), b"old executable")
        self.run_installer()
        self.assertEqual(list(self.destination.glob("tmd.backup.*")), backups)

    def test_bad_checksum_preserves_existing_installation(self):
        self.target.write_bytes(b"old")
        self.checksum.write_text("0" * 64 + "  ignored-name\n")
        self.run_installer(success=False)
        self.assertEqual(self.target.read_bytes(), b"old")
        self.assertEqual(list(self.destination.iterdir()), [self.target])

    def test_invalid_checksum_is_rejected(self):
        self.checksum.write_text("not-a-hash\n")
        self.run_installer(success=False)
        self.assertFalse(self.target.exists())

    def test_download_failure_preserves_existing_installation(self):
        self.target.write_bytes(b"old")
        self.env["TMD_TEST_DOWNLOAD_FAIL"] = "1"
        self.run_installer(success=False)
        self.assertEqual(self.target.read_bytes(), b"old")

    def test_wrong_binary_version_is_rejected(self):
        self.package(binary=b"#!/bin/sh\necho 'tmd 9.9.9'\n")
        self.run_installer(success=False)
        self.assertFalse(self.target.exists())

    def test_archive_paths_are_not_extracted(self):
        self.package(entry="../unexpected")
        self.run_installer(success=False)
        self.assertFalse((self.root / "unexpected").exists())
        self.assertFalse(self.target.exists())

    def test_symlink_target_is_not_replaced(self):
        other = self.root / "other"
        other.write_bytes(b"keep")
        self.target.symlink_to(other)
        self.run_installer(success=False)
        self.assertTrue(self.target.is_symlink())
        self.assertEqual(other.read_bytes(), b"keep")

    def test_directory_target_is_not_replaced(self):
        self.target.mkdir()
        self.run_installer(success=False)
        self.assertTrue(self.target.is_dir())

    def test_failed_atomic_replace_preserves_old_binary(self):
        self.target.write_bytes(b"old")
        self.command("mv", "#!/bin/sh\nexit 1\n")
        self.run_installer(success=False)
        self.assertEqual(self.target.read_bytes(), b"old")

    def test_rejects_unsupported_host_without_downloads(self):
        self.command("uname", "#!/bin/sh\necho Linux\n")
        self.run_installer(success=False)
        self.assertFalse(self.requests.exists())

    def test_rejects_invalid_tag_without_downloads(self):
        self.env["TMD_VERSION"] = "../../main"
        self.run_installer(success=False)
        self.assertFalse(self.requests.exists())

    def test_rejects_relative_directory(self):
        self.env["TMD_INSTALL_DIR"] = "relative"
        self.run_installer(success=False)
        self.assertFalse(self.requests.exists())

    def test_help_does_not_download_or_install(self):
        self.run_installer(args=("--help",))
        self.assertFalse(self.requests.exists())
        self.assertFalse(self.target.exists())


if __name__ == "__main__":
    unittest.main()
