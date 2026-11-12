# Gezi for macOS

This archive contains the equivalent `gezi` and `ge` Apple Silicon executables, MIT license and
`build-info.json` with its version, source commit and target. Rust, Python and
Lua are not required to run it.

From this extracted directory:

```sh
./gezi --version
./gezi help
./ge /path/to/document.md
./gezi /path/to/document.md
```

Press `/` to search, `o` for the outline, `c` to copy code, `?` for help and `q`
to exit. Editing and saving are planned; this build reads files without changing
them. Use `--remote-images off` for local images only, or `--images off` for text.

For installation, updates and full usage, see the
[Gezi repository](https://github.com/HiAriesZhou/Gezi).
Packages are ad hoc signed and are not Developer ID signed or notarized.
