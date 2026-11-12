#!/usr/bin/env python3
"""Build and inspect a fixed Apple Silicon archive using the standard library."""
import argparse
from dataclasses import dataclass
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import tarfile
import tempfile

from release_metadata import SHA, VERSION, ReleaseError, version_and_notes

TARGET = "aarch64-apple-darwin"
MAX_BYTES = 64 * 1024 * 1024
FILES = {"gezi": 0o755, "ge": 0o755, "LICENSE": 0o644, "README.md": 0o644, "build-info.json": 0o644}


@dataclass(frozen=True)
class Asset:
    path: Path
    size: int
    sha256: str

    @property
    def name(self):
        return self.path.name


def package_name(version):
    return f"gezi-v{version}-{TARGET}"


def check_macho(data):
    if (len(data) < 32 or data[:4] != bytes.fromhex("cffaedfe")
            or int.from_bytes(data[4:8], "little") != 0x0100000c
            or int.from_bytes(data[8:12], "little") != 0
            or int.from_bytes(data[12:16], "little") != 2):
        raise ReleaseError("Package needs an arm64 Mach-O executable")


def asset(path):
    if path.is_symlink() or not path.is_file():
        raise ReleaseError(f"Expected a regular asset: {path.name}")
    with path.open("rb") as source:
        data = source.read(MAX_BYTES + 1)
    if not data or len(data) > MAX_BYTES:
        raise ReleaseError(f"Invalid asset size: {path.name}")
    return Asset(path, len(data), hashlib.sha256(data).hexdigest())


def verify_package(directory, version, sha):
    if not isinstance(version, str) or not VERSION.fullmatch(version):
        raise ReleaseError("Package needs an exact SemVer version")
    if not SHA.fullmatch(sha):
        raise ReleaseError("Package needs an exact commit SHA")
    name = package_name(version)
    archive = asset(directory / (name + ".tar.gz"))
    checksum = asset(directory / (archive.name + ".sha256"))
    if {path.name for path in directory.iterdir()} != {archive.name, checksum.name}:
        raise ReleaseError("Asset directory contains unexpected files")
    expected_checksum = f"{archive.sha256}  {archive.name}\n".encode()
    if checksum.path.read_bytes() != expected_checksum:
        raise ReleaseError("Package checksum does not match the archive")
    try:
        with gzip.open(archive.path, "rb") as source:
            data = source.read(MAX_BYTES + 1)
        if len(data) > MAX_BYTES:
            raise ReleaseError("Expanded package exceeds the size limit")
        contents = {}
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:") as tar:
            for member in tar:
                path = member.name.removeprefix(name + "/")
                if (member.name != name + "/" + path or path not in FILES
                        or path in contents or not member.isfile()
                        or member.mode != FILES[path] or member.size > MAX_BYTES):
                    raise ReleaseError("Package contains an unexpected entry or mode")
                contents[path] = tar.extractfile(member).read()
            if any(data[tar.offset:]):
                raise ReleaseError("Package has unexpected trailing tar data")
        if set(contents) != set(FILES):
            raise ReleaseError("Package is missing required entries")
        check_macho(contents["gezi"])
        if contents["ge"] != contents["gezi"]:
            raise ReleaseError("Package commands must be identical")
        info = json.loads(contents["build-info.json"])
        if info != {"version": version, "commit": sha, "target": TARGET}:
            raise ReleaseError("Package build metadata does not match the checked commit")
        if not contents["LICENSE"].strip() or not contents["README.md"].strip():
            raise ReleaseError("Package needs its license and usage instructions")
    except (OSError, EOFError, tarfile.TarError, ValueError, UnicodeError):
        raise ReleaseError("Could not read the release package") from None
    return (archive, checksum)


def create_package(root, binary, directory, sha):
    version, _ = version_and_notes(root)
    if not SHA.fullmatch(sha):
        raise ReleaseError("Package needs an exact commit SHA")
    mode = binary.lstat().st_mode
    if not stat.S_ISREG(mode) or not mode & 0o111:
        raise ReleaseError("Package input must be a regular executable")
    with binary.open("rb") as source:
        payload = source.read(MAX_BYTES + 1)
    check_macho(payload)
    if len(payload) > MAX_BYTES:
        raise ReleaseError("Executable exceeds the package size limit")
    directory.mkdir(parents=True, exist_ok=True)
    name = package_name(version)
    archive = directory / (name + ".tar.gz")
    checksum = directory / (archive.name + ".sha256")
    if archive.exists() or archive.is_symlink() or checksum.exists() or checksum.is_symlink():
        raise ReleaseError("Package output already exists; choose an empty output directory")
    # Check exactly the bytes that will be archived, even if a build replaces
    # the input path during validation. No user shell or config is loaded.
    with tempfile.TemporaryDirectory(prefix="gezi-package-check-") as temporary:
        temporary = Path(temporary)
        executable = temporary / "gezi"
        executable.write_bytes(payload)
        executable.chmod(0o755)
        env = dict(os.environ, XDG_CONFIG_HOME=str(temporary / "config"), PATH="/usr/bin:/bin")
        result = subprocess.run([str(executable), "--version"], env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
        if result.returncode or result.stdout.strip() != f"gezi {version}".encode():
            raise ReleaseError("Package executable version does not match Cargo")
    def document(path):
        if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
            raise ReleaseError("Package documents must be regular files inside the source tree")
        asset(path)
        return path.read_bytes()

    contents = {
        "gezi": payload,
        "ge": payload,
        "LICENSE": document(root / "LICENSE"),
        "README.md": document(root / "docs/binary-package.md"),
        "build-info.json": (json.dumps({"version": version, "commit": sha, "target": TARGET},
                                       indent=2) + "\n").encode(),
    }
    # Fixed metadata and gzip timestamp make identical inputs reproducible.
    with archive.open("xb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode="w|") as tar:
            for path, data in contents.items():
                entry = tarfile.TarInfo(name + "/" + path)
                entry.size = len(data)
                entry.mode = FILES[path]
                tar.addfile(entry, io.BytesIO(data))
    with checksum.open("x") as output:
        output.write(f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n")
    return verify_package(directory, version, sha)


def checked_commit(root):
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    if not SHA.fullmatch(sha):
        raise ReleaseError("Expected an exact checked source commit")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=root):
        raise ReleaseError("Package source must be a clean committed checkout")
    if os.environ.get("GITHUB_SHA", sha) != sha:
        raise ReleaseError("Package source is not the workflow's exact checked commit")
    return sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    try:
        if platform.system() != "Darwin" or platform.machine() != "arm64":
            raise ReleaseError("Build the package on an Apple Silicon macOS runner")
        sha = checked_commit(root)
        for item in create_package(root, args.binary, args.out_dir, sha):
            print(f"Created {item.name}: sha256 {item.sha256}")
        return 0
    except (ReleaseError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Package failed: {error}", file=__import__("sys").stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
