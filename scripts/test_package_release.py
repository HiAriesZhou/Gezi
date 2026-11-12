"""Check archive boundaries and reproducibility; native execution runs separately."""
from copy import deepcopy
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import package_release as package
from release_metadata import ReleaseError

SHA = "a" * 40
VERSION = "0.4.0"


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="gezi-package-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "cli").mkdir()
        (self.root / "docs").mkdir()
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers=["cli"]\n[workspace.package]\nversion="0.4.0"\n')
        (self.root / "cli/Cargo.toml").write_text('[package]\nname="tmd-cli"\nversion.workspace=true\n')
        (self.root / "Cargo.lock").write_text('[[package]]\nname="tmd-cli"\nversion="0.4.0"\n')
        (self.root / "CHANGELOG.md").write_text('## 0.4.0\n\nPackage test notes.\n')
        (self.root / "LICENSE").write_text('MIT License\n')
        (self.root / "docs/binary-package.md").write_text('Run ./gezi --version\n')
        self.binary = self.root / "gezi"
        self.payload = (bytes.fromhex('cffaedfe') + (0x0100000c).to_bytes(4, 'little')
                        + bytes(4) + (2).to_bytes(4, 'little') + bytes(16) + b'test payload')
        self.binary.write_bytes(self.payload)
        self.binary.chmod(0o755)
        self.native = patch.object(package.subprocess, 'run', return_value=
                                   subprocess.CompletedProcess([], 0, b'gezi 0.4.0\n', b''))
        self.run = self.native.start()
        self.addCleanup(self.native.stop)

    def create(self, name='output'):
        directory = self.root / name
        package.create_package(self.root, self.binary, directory, SHA)
        return directory

    def rewrite(self, directory, transform):
        archive = directory / (package.package_name(VERSION) + '.tar.gz')
        with package.tarfile.open(archive) as tar:
            entries = [(deepcopy(member), tar.extractfile(member).read()) for member in tar]
        entries = transform(entries)
        with archive.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as gz:
            with package.tarfile.open(fileobj=gz, mode='w|') as tar:
                for member, content in entries:
                    member.size = len(content)
                    tar.addfile(member, io.BytesIO(content) if member.isfile() else None)
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        (directory / (archive.name + '.sha256')).write_text(f'{digest}  {archive.name}\n')

    def test_same_inputs_make_identical_archives_and_exact_allowlist(self):
        first = package.verify_package(self.create('first'), VERSION, SHA)
        second = package.verify_package(self.create('second'), VERSION, SHA)
        self.assertEqual(first[0].path.read_bytes(), second[0].path.read_bytes())
        self.assertEqual(first[0].sha256, second[0].sha256)
        with package.tarfile.open(first[0].path) as tar:
            self.assertEqual({member.name for member in tar},
                             {package.package_name(VERSION) + '/' + path for path in package.FILES})
            self.assertEqual(tar.extractfile(package.package_name(VERSION) + '/ge').read(),
                             tar.extractfile(package.package_name(VERSION) + '/gezi').read())
            info = json.load(tar.extractfile(package.package_name(VERSION) + '/build-info.json'))
            self.assertEqual(info, {'version': VERSION, 'commit': SHA, 'target': package.TARGET})
        command = self.run.call_args.args[0]
        self.assertEqual(command[1:], ['--version'])
        self.assertEqual(self.run.call_args.kwargs['env']['PATH'], '/usr/bin:/bin')

    def test_short_command_cannot_be_missing_or_different(self) -> None:
        def missing(entries):
            return [(member, data) for member, data in entries if not member.name.endswith('/ge')]

        def different(entries):
            return [(member, b'other tool' if member.name.endswith('/ge') else data)
                    for member, data in entries]

        for name, transform, message in [('missing', missing, 'missing'),
                                         ('different', different, 'identical')]:
            with self.subTest(name=name):
                directory = self.create(name)
                self.rewrite(directory, transform)
                with self.assertRaisesRegex(ReleaseError, message):
                    package.verify_package(directory, VERSION, SHA)

    def test_invalid_input_architecture_mode_symlink_and_native_version(self):
        self.binary.chmod(0o644)
        with self.assertRaises(ReleaseError):
            self.create()
        self.binary.chmod(0o755)
        self.binary.write_bytes(bytes(64))
        with self.assertRaisesRegex(ReleaseError, 'Mach-O'):
            self.create()
        self.binary.write_bytes(self.payload)
        link = self.root / 'link'
        link.symlink_to(self.binary)
        with self.assertRaisesRegex(ReleaseError, 'regular executable'):
            package.create_package(self.root, link, self.root / 'link-out', SHA)
        self.run.return_value = subprocess.CompletedProcess([], 0, b'gezi 0.0.0\n', b'')
        with self.assertRaisesRegex(ReleaseError, 'version'):
            self.create()

    def test_outputs_and_document_symlinks_are_not_overwritten_or_followed(self):
        directory = self.create()
        original = {p.name: p.read_bytes() for p in directory.iterdir()}
        with self.assertRaisesRegex(ReleaseError, 'already exists'):
            self.create()
        self.assertEqual(original, {p.name: p.read_bytes() for p in directory.iterdir()})
        license_path = self.root / 'LICENSE'
        license_path.unlink()
        license_path.symlink_to(self.root / 'Cargo.toml')
        with self.assertRaisesRegex(ReleaseError, 'regular files'):
            self.create('linked-document')

    def test_bad_checksum_missing_or_extra_asset_is_rejected(self):
        directory = self.create()
        checksum = next(directory.glob('*.sha256'))
        valid = checksum.read_bytes()
        checksum.write_text('wrong\n')
        with self.assertRaisesRegex(ReleaseError, 'checksum'):
            package.verify_package(directory, VERSION, SHA)
        checksum.write_bytes(valid)
        extra = directory / 'unexpected'
        extra.write_text('not an asset')
        with self.assertRaisesRegex(ReleaseError, 'unexpected'):
            package.verify_package(directory, VERSION, SHA)
        extra.unlink()
        checksum.unlink()
        with self.assertRaises(ReleaseError):
            package.verify_package(directory, VERSION, SHA)

    def test_archive_metadata_and_entry_boundaries_after_valid_checksum(self):
        baseline = self.create('baseline')

        def change_member(field, value):
            def transform(entries):
                setattr(entries[0][0], field, value)
                return entries
            return transform

        def bad_info(entries):
            for index, (member, content) in enumerate(entries):
                if member.name.endswith('/build-info.json'):
                    info = json.loads(content)
                    info['commit'] = 'b' * 40
                    entries[index] = (member, json.dumps(info).encode())
            return entries

        transforms = [
            lambda entries: entries[:-1],
            lambda entries: entries + [deepcopy(entries[0])],
            change_member('name', '../outside'),
            change_member('name', '/outside'),
            change_member('mode', 0o777),
            change_member('type', package.tarfile.SYMTYPE),
            bad_info,
        ]
        for index, transform in enumerate(transforms):
            with self.subTest(index=index):
                directory = self.root / f'bad-{index}'
                shutil.copytree(baseline, directory)
                self.rewrite(directory, transform)
                with self.assertRaises(ReleaseError):
                    package.verify_package(directory, VERSION, SHA)
        self.assertFalse((self.root.parent / 'outside').exists())
        package.verify_package(baseline, VERSION, SHA)

    def test_expansion_is_bounded_and_wrong_version_is_rejected_before_paths(self):
        directory = self.create()
        with patch.object(package, 'MAX_BYTES', 16384):
            package.verify_package(directory, VERSION, SHA)
        def oversized(entries):
            member, _ = entries[2]
            entries[2] = (member, b'x' * 50000)
            return entries
        self.rewrite(directory, oversized)
        with patch.object(package, 'MAX_BYTES', 16384), self.assertRaisesRegex(ReleaseError, 'size limit'):
            package.verify_package(directory, VERSION, SHA)
        for version in ('../outside', 'v0.4.0', '0.04.0'):
            with self.subTest(version=version), self.assertRaisesRegex(ReleaseError, 'SemVer'):
                package.verify_package(directory, version, SHA)

    def test_source_commit_must_be_clean_and_match_the_workflow(self):
        with patch.dict(package.os.environ, {"GITHUB_SHA": SHA}):
            with patch.object(package.subprocess, "check_output", side_effect=[SHA + "\n", b""]):
                self.assertEqual(package.checked_commit(self.root), SHA)
            with patch.object(package.subprocess, "check_output", side_effect=[SHA + "\n", b" M file\n"]):
                with self.assertRaisesRegex(ReleaseError, "clean committed"):
                    package.checked_commit(self.root)
            with patch.object(package.subprocess, "check_output", side_effect=["b" * 40 + "\n", b""]):
                with self.assertRaisesRegex(ReleaseError, "exact checked"):
                    package.checked_commit(self.root)


if __name__ == '__main__':
    unittest.main()
