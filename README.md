# tmd

A Kaku-first Markdown editor for the terminal, built for developer and AI workflows.

**In development:** current releases deliver the reading and rendering foundation.
Editing, undo/redo and saving files are next. macOS Apple Silicon is supported.

## Install

Install the latest release, or run the same command to update:

```sh
curl -fsSL https://raw.githubusercontent.com/HiAriesZhou/TMD/main/install.sh | sh
```

The installer verifies SHA256, installs to `~/.local/bin`, and keeps a backup when
replacing an existing binary. No Rust, sudo or shell configuration changes are
needed. If the directory is not on your PATH, add this line to `~/.zshrc` and run
it in your current shell:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

To choose a release or directory, set `TMD_VERSION=v0.1.4` or
`TMD_INSTALL_DIR=/absolute/path` on the `sh` command. You can also inspect
[install.sh](install.sh) or download the archive and checksum from
[Releases](https://github.com/HiAriesZhou/TMD/releases/latest).
Packages are ad hoc signed, without Developer ID signing or notarization.

<details>
<summary>Build from source</summary>

With Git and a stable Rust toolchain:

```sh
cargo install --git https://github.com/HiAriesZhou/TMD.git --locked tmd-cli
```

Cargo installs the `tmd` command in `~/.cargo/bin`. Use `--force` to reinstall.
If you also use the binary installer, check `command -v tmd` to see which copy
runs.

</details>

## Quick start

```sh
tmd README.md                        # Open the interactive reader
cat notes.md | tmd                    # Render a Markdown stream
tmd --ansi README.md                 # Print once and return to the shell
tmd --remote-images off README.md    # Load local images only
tmd --theme neon-tokyo README.md     # Choose a theme
tmd --help
```

File reading is interactive when stdin and stdout are terminals. Press `q` to
exit. Use `--reader inline` if you previously configured native output.

| Key | Action |
| --- | --- |
| Scroll wheel, ↑ / ↓, `j` / `k` | Scroll |
| `/`, then Enter; `n` / `N` | Search; next / previous match |
| `o`; Tab | Toggle the outline; switch panes |
| `c` | Choose and copy a code block |
| `?` | Help for the current view |

## Available today

- Markdown rendering with highlighted code, tables, links, lists and mixed Chinese/English layout.
- Interactive reading with search, a heading sidebar, repeated navigation and a responsive footer.
- Code copying that preserves indentation and line breaks, without rendered borders.
- Local and remote PNG, JPEG and static SVG images in direct Kaku sessions, including HTML `<img>` and badge rows.
- Incremental Markdown output and a Claude Code stream-json adapter: `cat events.jsonl | tmd --from claude-json`.
- Automatic light/dark themes, custom TOML themes and optional large H1 headings in Kaku.
- Main-screen reading that preserves terminal history and restores terminal input on exit.

Remote images load by default and contact the hosts in the document. Use
`--remote-images off` to disable remote loading or `--images off` to disable all
image loading. Graphics require the inline reader in direct Kaku; native output,
streams and other terminals keep text labels. Code copying requires local macOS.

## TODO

Editing is planned; these capabilities are not in the current release. In order:

- [ ] Editable Markdown source with cursor movement, selection, Chinese input and undo/redo.
- [ ] Safe saving, unsaved-change prompts and detection of external file changes.
- [ ] Switch between source and rendered views while keeping the reading/editing position.
- [ ] Find and replace, followed by task-list and code-block editing shortcuts.

## Documentation

- [Reader guide](docs/reader.md): navigation, images and optional Kaku integration.
- [Themes](themes/README.md): built-in palettes and custom themes.
- [Changelog](CHANGELOG.md): changes by release.

For contributors: [architecture](docs/architecture.md),
[testing](docs/terminal-testing.md), [release maintenance](docs/release.md)
and [repository rules](AGENTS.md). Bug reports should include the tmd version,
terminal and a minimal Markdown example.

[MIT License](LICENSE)
