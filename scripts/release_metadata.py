"""Shared version and changelog checks for packages and publication."""
import re
import tomllib


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
