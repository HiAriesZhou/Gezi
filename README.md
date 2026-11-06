# tmd

A Kaku-first Markdown reader for the terminal, built for developer documentation
and AI output. Read files, search their contents, or render a stream as it arrives.

tmd is an early, read-only tool written in Rust. It focuses on streaming without
redrawing completed content, mixed Chinese and English text, and terminal themes.
Kaku is the primary terminal for hands-on testing.

## Install from source

You need Git and a stable Rust toolchain. Releases currently provide source
archives; prebuilt packages are not available.

These commands install tmd in `~/.local/bin` and replace an existing binary at
that location:

```sh
git clone https://github.com/HiAriesZhou/TMD.git tmd
cd tmd
cargo build --release --locked
mkdir -p "$HOME/.local/bin"
install -m 755 target/release/tmd "$HOME/.local/bin/tmd"
```

If that directory is not on your PATH, run the following. Add the `export` line
to your shell configuration to keep it across sessions.

```sh
export PATH="$HOME/.local/bin:$PATH"
tmd --version
```

## Usage

```sh
tmd README.md          # Read a Markdown file
cat notes.md | tmd     # Render Markdown from stdin
tmd --ansi README.md   # Print once, without the interactive reader
tmd --help             # Show all options
```

Supported Markdown includes headings, emphasis, lists, blockquotes, links,
highlighted code blocks, and tables with mixed Chinese and English text.
Headings use color, weight, and separators by default.

When reading a file in an interactive terminal, tmd opens at the beginning and
uses the current screen. It preserves terminal history and stays open for
nonempty documents.

| Control | Action |
| --- | --- |
| Mouse wheel, arrow keys, `j` / `k` | Scroll |
| Home / End | Go to the beginning or end |
| PageUp / PageDown, `b` / Space | Move by a page |
| `/`, then Enter | Search visible text, including headings and code |
| `n` / `N` | Jump to the next or previous match |
| `?` | Show reading help |
| `q` | Return to the shell |

Search is case-sensitive and supports Chinese and emoji. Esc cancels a query
being edited; while reading, it clears active search marks before exiting on a
second press. In the help view, Esc closes help. Empty documents exit immediately.

No configuration is required. `--reader inline|native` and the optional
`reader.mode` setting select the reading mode. Native Kaku positioning needs the
adapter in `contrib/kaku-reader.lua`; if the adapter is unavailable, tmd falls
back to its built-in reader. See the [reader guide](docs/reader.md) for setup,
text selection, and exit behavior.

Piped Markdown renders incrementally. For Claude Code stream-json event files,
select the JSONL adapter:

```sh
cat events.jsonl | tmd --from claude-json
python3 scripts/slowcat.py fixtures/acceptance.md --rate 600 | tmd
```

Choose a theme or set the output width:

```sh
tmd --theme neon-tokyo README.md
tmd --width 80 fixtures/acceptance.md
```

Built-in themes are `default-dark`, `default-light`, and `neon-tokyo`. The default
`auto` setting chooses a light or dark theme based on the terminal background.
See [themes](themes/README.md) for custom TOML themes.

For static files directly in Kaku, experimental large headings double the width
and height of top-level H1 headings. Other headings and body text keep their
normal size:

```sh
tmd --large-headings fixtures/large-headings.md
```

This option is off by default. Piped input, redirected output, `--ansi`, Claude
JSONL, tmux/screen, and other terminals retain normal headings. Large headings
need at least four columns and three rows; an explicit `--width` must fit the
terminal and be at least four columns. Copying a double-height heading through
the terminal can duplicate its text, so use normal headings when copying.

## Current limitations

- tmd is a reader, not an editor, and does not display images.
- Kaku is the primary test environment. Visual behavior in Ghostty and kitty
  still needs hands-on validation.
- Claude JSONL has replay tests using sanitized event samples; live incremental
  Claude pipelines still need further validation.
- During streaming, resize only reflows the active area. Completed output in
  terminal history is not redrawn.
- Large headings are limited to static files and do not provide a separate font
  size for each heading level.

See the [changelog](CHANGELOG.md) for release history. The detailed reader,
theme, architecture, testing, and release guides are currently in Chinese.

## Development

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The workspace has two crates: `tmd-core` handles parsing and layout; `tmd-cli`
handles input and terminal control. See [architecture](docs/architecture.md),
[terminal testing](docs/terminal-testing.md), and [contributor rules](AGENTS.md).

Changes go through pull requests with Linux and macOS checks. Version updates
publish automatically after main CI passes, so they require maintainer approval
before merging. See [release maintenance](docs/release.md).

Report bugs with the tmd version, terminal, operating system, and a minimal
Markdown example. Remove private document content and credentials before sharing
samples. Keep task plans, handoffs, and discussion records outside the repository.

## License

[MIT](LICENSE)
