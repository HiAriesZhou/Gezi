#!/usr/bin/env python3
"""Extract the verified archive and run its actual CLI in an isolated config."""
import argparse
import os
from pathlib import Path
import platform
import subprocess
import sys
import tarfile
import tempfile

from package_release import checked_commit, package_name, verify_package
from release_metadata import ReleaseError, version_and_notes


def smoke(root, directory):
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise ReleaseError("Package smoke needs a native Apple Silicon macOS runner")
    version, _ = version_and_notes(root)
    assets = verify_package(directory, version, checked_commit(root))
    with tempfile.TemporaryDirectory(prefix="tmd-installed-package-") as temporary:
        temporary = Path(temporary)
        with tarfile.open(assets[0].path) as archive:
            archive.extractall(temporary, filter="data")
        binary = temporary / package_name(version) / "tmd"
        env = dict(os.environ, XDG_CONFIG_HOME=str(temporary / "config"), PATH="/usr/bin:/bin")
        def output(*arguments):
            return subprocess.check_output([str(binary), *arguments], env=env, timeout=30)
        if output("--version").strip() != f"tmd {version}".encode():
            raise ReleaseError("Installed executable version does not match the package")
        if b"Usage:" not in output("--help"):
            raise ReleaseError("Installed executable help is missing")
        rendered = output("--ansi", "--theme", "default-dark", str(root / "fixtures/acceptance.md"))
        if "中文".encode() not in rendered or len(rendered) < 100:
            raise ReleaseError("Installed executable did not render the Markdown fixture")
        for script in ("test_reader_pty.py", "check_stream_pty.py", "test_stream_pty.py"):
            result = subprocess.run([sys.executable, "-B", str(root / "scripts" / script),
                                     "--binary", str(binary)], env=env, timeout=180,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if result.returncode:
                detail = result.stderr[-2000:].decode(errors="replace")
                raise ReleaseError(f"Installed package {script} failed: {detail}")
    return "Installed package version/help/render and reader/stream PTY checks passed"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-dir", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(smoke(Path(__file__).resolve().parents[1], args.package_dir))
        return 0
    except (ReleaseError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Package smoke failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
