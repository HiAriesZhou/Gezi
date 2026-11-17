# Installation and updates

Gezi supports macOS Apple Silicon. The binary package contains identical `gezi`
and `ge` executables and needs no Rust, Python or Lua at runtime. Packages are
ad hoc signed, without Developer ID signing or notarization.

## Binary installation

```sh
curl -fsSL https://raw.githubusercontent.com/HiAriesZhou/Gezi/main/install.sh | sh
```

The installer verifies SHA256, installs to `~/.local/bin` and backs up existing
Gezi binaries. It does not use sudo or change shell configuration. If the directory
is not on PATH, add this line to `~/.zshrc` and run it in the current shell:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

To choose a release or directory, set `GEZI_VERSION=v0.1.14` or
`GEZI_INSTALL_DIR=/absolute/path` on the `sh` command. You can inspect
[install.sh](../install.sh) before running it, or download the archive and checksum
from [Releases](https://github.com/HiAriesZhou/Gezi/releases/latest).
The [archive instructions](binary-package.md) explain how to run an extracted package.

Verify the installation:

```sh
gezi --version
ge --version
gezi help
gezi /path/to/document.md
```

## Updating a binary installation

```sh
gezi update
# Equivalent: ge update
```

The updater uses macOS's curl and archive/checksum tools to fetch the latest
stable release. It verifies SHA256, keeps a backup and replaces both commands
beside the running executable. Equal or older available versions leave the
installation unchanged. It does not require sudo.

Do not mix Cargo and binary-package installations in the same directory. When
switching methods, use a separate directory or uninstall the Cargo package first.
The installer refuses to overwrite an unrelated `ge` command.

## Source installation

The Cargo package name is `tmd-cli`; its commands are `gezi` and `ge`:

```sh
cargo install --git https://github.com/HiAriesZhou/Gezi.git --locked tmd-cli
```

Cargo installs both commands in `~/.cargo/bin`. Update by repeating the command
with `--force`. The built-in `gezi update` is for binary-package installations;
it does not manage Cargo installations.

## Install with a coding agent

Copy this prompt into your coding agent:

```text
Install the latest stable Gezi release from
https://github.com/HiAriesZhou/Gezi on this machine.

1. Check for Apple Silicon macOS and a native arm64 shell; stop if unsupported.
2. Check existing gezi/ge commands, aliases, symlinks and installation methods.
   Do not overwrite another tool or mix Cargo and binary-package installations.
   Explain any conflict before changing files.
3. Download the repository's main/install.sh to a temporary file, inspect it,
   then run it with sh. Keep SHA256 verification, use ~/.local/bin and no sudo.
   Preserve existing configuration and unrelated files.
4. Ensure ~/.local/bin is on PATH. Explain any required shell startup change;
   add only the missing entry without replacing existing settings.
5. Verify both commands resolve to this installation and report the same version.
   Run gezi help and render a short Markdown sample.
6. Report the paths and version, how to open a file, and how to use gezi update
   or ge update. Stop and report errors rather than claiming success.
```
