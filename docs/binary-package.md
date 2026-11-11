# tmd for macOS

This archive contains the Apple Silicon executable, MIT license and
`build-info.json` with its version, source commit and target. Rust, Python and
Lua are not required to run it.

From this extracted directory:

```sh
./tmd --version
./tmd help
./tmd /path/to/document.md
```

Press `/` to search, `o` for the outline, `c` to copy code, `?` for help and `q`
to exit. Editing and saving are planned; this build reads files without changing
them. Use `--remote-images off` for local images only, or `--images off` for text.

For installation, updates and full usage, see the
[tmd repository](https://github.com/HiAriesZhou/TMD).
Packages are ad hoc signed and are not Developer ID signed or notarized.
