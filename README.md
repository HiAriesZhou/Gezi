# Gezi

A Kaku-first Markdown editor for the terminal, built for developer and AI workflows.

Gezi (格子, “grid” in Chinese) takes its name from manuscript paper and the
terminal’s character grid. Use `gezi` or the shorter `ge`; both run the same editor.

**In development:** current releases deliver the reading and rendering foundation.
Editing, undo/redo and saving files are next. macOS Apple Silicon is supported.

## Install

Install the latest release:

```sh
curl -fsSL https://raw.githubusercontent.com/HiAriesZhou/Gezi/main/install.sh | sh
```

The installer verifies SHA256, installs to `~/.local/bin`, and keeps a backup when
replacing an existing binary. No Rust, sudo or shell configuration changes are
needed. If the directory is not on your PATH, add this line to `~/.zshrc` and run
it in your current shell:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

To choose a release or directory, set `GEZI_VERSION=v0.1.7` or
`GEZI_INSTALL_DIR=/absolute/path` on the `sh` command. You can also inspect
[install.sh](install.sh) or download the archive and checksum from
[Releases](https://github.com/HiAriesZhou/Gezi/releases/latest).
Packages are ad hoc signed, without Developer ID signing or notarization.

## Update

Update to the latest stable release:

```sh
gezi update
# Equivalent short command:
ge update
```

The updater verifies SHA256, backs up the previous binary and updates both
commands beside the running executable. It does nothing when the installed
version is already current or newer. It does not require sudo.

The built-in updater supports macOS binary-package installations. For Cargo
source builds, use the [source installation command](docs/architecture.md#source-installation)
with `--force`. To switch installation methods, use a separate directory or
uninstall the Cargo package first.

## Quick start

```sh
gezi README.md                       # Open the interactive reader
ge README.md                         # Short form of the same command
cat notes.md | gezi                    # Render a Markdown stream
gezi --ansi README.md                 # Print once and return to the shell
gezi --remote-images off README.md    # Load local images only
gezi --theme neon-tokyo README.md     # Choose a theme
gezi help
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
- Incremental Markdown output and a Claude Code stream-json adapter: `cat events.jsonl | gezi --from claude-json`.
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
and [repository rules](AGENTS.md). Bug reports should include the gezi version,
terminal and a minimal Markdown example.

[MIT License](LICENSE)
