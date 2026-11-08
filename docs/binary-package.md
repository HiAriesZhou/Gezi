# tmd for macOS

This package runs on Apple Silicon Macs. It contains the `tmd` executable, the
MIT license, and `build-info.json` with the version, source commit, and target
platform. You do not need Rust, Python, or Lua to run it. Native Kaku positioning
is an optional integration; the built-in reader works without configuration.

Download the `.tar.gz` archive and its `.sha256` file from the same release.
In the download directory, verify the checksum before extracting. For 0.1.3:

```sh
shasum -a 256 -c tmd-v0.1.3-aarch64-apple-darwin.tar.gz.sha256
tar -xzf tmd-v0.1.3-aarch64-apple-darwin.tar.gz
cd tmd-v0.1.3-aarch64-apple-darwin
./tmd --version
./tmd --help
./tmd article.md
```

Press `/` to search, `c` to choose a code block to copy, and `?` for reading help.
Press `q` to return to the shell. Use `./tmd --ansi article.md` for one-shot output.

To install in your personal command directory:

```sh
mkdir -p "$HOME/.local/bin"
install -m 755 tmd "$HOME/.local/bin/tmd"
export PATH="$HOME/.local/bin:$PATH"
tmd --version
```

This replaces an existing `tmd` at that location. Save the previous executable
first if you need to keep it. Add the PATH line to your shell configuration to
keep it across sessions.

The package targets Apple Silicon macOS. Intel Mac binaries and Homebrew
installation are not provided. Packages are not Developer ID signed or notarized.
Source code, configuration, and limitations are documented in the
[tmd repository](https://github.com/HiAriesZhou/TMD).
