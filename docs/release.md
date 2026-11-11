# Releases

## Version policy

Use `0.1.x` for ordinary features, improvements and fixes. Increment the third
component, for example `0.1.1` to `0.1.2`. Change the second component only for a
major change explicitly approved by the maintainer; completing a milestone is
not sufficient.

Each independently delivered task gets its own patch version, in completion
order. Its pre-release reviews, fixes, tests and documentation belong to that
version. Preserve these boundaries when rebuilding historical releases; do not
renumber the current combined feature set as an older candidate. Unverified or
unpublished historical candidates are not released versions.

## Publishing

1. Update `[workspace.package].version` in `Cargo.toml` and both project packages in `Cargo.lock`.
2. Add exactly one nonempty `## <version>` section to `CHANGELOG.md`.
3. Open a PR, complete review and obtain maintainer confirmation before merging to `main`.
4. After main's macOS CI passes, `Publish version` uploads and verifies the Apple Silicon package and SHA256, pins `v<version>` to that checked commit, then publishes the GitHub Release.

The title is `tmd v<version>`; the body is only that version's changelog section.
Versions such as `0.1.0-rc.1` are prereleases. An unchanged version that already
has a published Release is skipped.

Releases contain a macOS Apple Silicon `.tar.gz`, its `.sha256` and GitHub's
source archives. Intel binaries, crates.io and Homebrew packages are not provided.

## Building a candidate

In a clean, committed checkout on Apple Silicon macOS:

```sh
cargo build --release --locked
python3 -B scripts/package_release.py --binary target/release/tmd --out-dir target/release-package
python3 -B scripts/check_package.py --package-dir target/release-package
```

Packaging verifies architecture and version, then creates
`tmd-v<version>-aarch64-apple-darwin.tar.gz` and its checksum. The archive contains
the executable, license, [package instructions](binary-package.md) and
version/commit/target metadata. Running it requires neither Rust nor Lua.
Identical inputs use fixed archive metadata. Existing outputs are rejected;
choose a new empty destination.

The checker extracts the actual binary and tests version, help, Chinese rendering
and reader/image/stream PTY behavior with isolated configuration and a PATH
containing only system tools. These development checks need Python; the shipped
executable does not.

macOS CI performs the same build and checks, then uploads the package as a run
artifact. Publishing downloads only the same run's artifact and checks its commit
against `github.sha`. Re-running checks replaces that run's artifact. Package
output lives in runner temporary storage, not the Rust build cache.

## Publication boundaries

- Withdrawing old Releases or tags requires separate maintainer confirmation. Never move or reuse an existing tag or rewrite commit history as routine release work.
- Check and publish jobs both use macOS. Publishing depends on successful checks and only accepts a push to `refs/heads/main`; PRs, other branches and tag events do not publish.
- Checks have `contents: read`. The separate publisher temporarily receives `contents: write`, checks out the exact run commit and does not persist credentials.
- The API pins the tag to the checked commit. If main has advanced before creation, skip and let the newer commit's run publish.
- Publishers run serially. An existing published Release of the same version is skipped without modification.
- A tag without a Release can be completed only when it points to the exact run commit. A different target or an existing draft is an error requiring maintainer inspection.
- Publish only a draft created by this run. After uploading both assets, download and compare their bytes, then recheck main and draft ownership before publication. Never adopt a pre-existing draft.
- A timed-out write may have succeeded. Read back remote state and verify the run's ownership marker; if uncertain, stop without repeating creation, upload or publication. Remove ownership markers from the final public release notes.

## Verification and recovery

```sh
python3 -B -m unittest discover -s scripts -p 'test_*release.py'
```

Tests use simulated GitHub responses and do not create remote tags or Releases.
CI runs them too. After a failure, inspect the publisher log, Release, tag and
assets before acting. A rerun skips completed publication; it will not take over
a leftover draft. Have the maintainer resolve a draft or conflict before retrying
failed jobs. Never delete or move a published tag to reuse its version.

Implementation references: [job dependencies](https://docs.github.com/en/actions/how-tos/write-workflows/choose-what-workflows-do/use-jobs)
and [Release API](https://docs.github.com/en/rest/releases/releases#create-a-release).
