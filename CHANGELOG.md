# Changelog

## 0.1.8

- Run `gezi update` or `ge update` to update a macOS binary-package installation to the latest stable release. Verify downloads, update both commands in the running executable’s directory, keep a backup and leave equal or newer installations unchanged.
- Simplify the README around `gezi` and `ge`, with dedicated installation and update instructions.

## 0.1.7

- Rename the product to Gezi (格子) and provide equivalent `gezi` and `ge` commands. The GitHub repository is now `Gezi`. Earlier releases retain their TMD names.
- Ship both commands in macOS packages and the installer. Preserve an existing `tmd` binary, refuse to overwrite another tool’s `ge`, and continue to accept legacy installer environment variables.
- Read configuration and themes from `gezi/` first, with a per-file fallback to `tmd/`. Existing Kaku native-reader adapters remain compatible.

## 0.1.6

- Pressing Ctrl-C or sending TERM or HUP while streaming or printing large headings now ends tmd by that signal after the terminal is restored, so a shell script or loop around tmd stops as well. QUIT still exits with status 131.
- An idle stream waiting for input no longer wakes about 60 times per second.

## 0.1.5

- Run `tmd help` for full command-line help without entering the reader or waiting for stdin. `--help` and `-h` remain supported; use `./help` or `-- help` for a file named `help`.
- Use English throughout project documentation and repository rules, with updated help examples. Multilingual rendering fixtures remain unchanged.

## 0.1.4

View document images directly in Kaku's built-in reader.

- Display local and remote PNG, JPEG and static SVG from Markdown images and HTML `<img>`. Preserve proportions through scrolling, sidebar navigation and resizing; clean up only this reader's image IDs on exit.
- Load remote images in the background by default. `--remote-images off` keeps local images; `--images off` uses text labels. Failures retain alt text, and slow requests do not block navigation or exit.

Apple Silicon macOS only. Graphics require direct Kaku with Kitty support enabled.
Animation, table-cell graphics, streaming images and other terminal backends are
not included.

## 0.1.3

Navigate long documents with an outline sidebar and focus-specific controls.

- Press `o` to navigate body H1-H6, with separate current-section and selection indicators, duplicate/empty headings and source line numbers. At 80 columns, show sidebar and document together; Enter keeps outline focus, Tab switches panes and the wheel follows its pane. Narrow windows use a full-page list; resize preserves selection and reading position.
- Reading, search, outline, code and help have their own footers. Narrow windows prioritize confirmation and return actions. Help restores the previous view and list selection.
- Jump, copy-success and empty-outline feedback expires automatically. Copy errors persist until selection changes, retry or cancellation. Outline navigation preserves search.

Split views use ordinary headings; closing the sidebar restores requested large
headings. macOS only. The inline outline does not filter, fold, resize by dragging
or open automatically at startup.

## 0.1.2

Choose a code block in the inline reader and copy its original contents.

- Press `c` for the code chooser and Enter to copy, preserving tabs, trailing spaces, blank lines and the final newline. Includes code inside lists and blockquotes.
- Download an Apple Silicon executable archive and SHA256 file without installing Rust. The archive includes instructions, license and version/commit metadata.

Copying uses the local macOS clipboard and normalizes CRLF to LF. Empty blocks do
not alter the clipboard; q/Esc cancels and restores the reading position. SSH and
other systems are not supported.

## 0.1.1

Search documents in the inline reader and move between matches.

- Press `/` to enter English, Chinese or emoji, then Enter to locate and mark the matching line. n/N wraps through matches; Esc while editing cancels and restores the reading position.

Search is case-sensitive and independent of soft wrapping. While reading, Esc
clears search marks before a second press exits.

## 0.1.0

Keep short Markdown documents open and access reading help at any time.

- Nonempty documents no longer exit automatically. Enlarging the window keeps interaction active; q/Esc returns to the shell.
- Press `?` for scrolling, paging and selection help; closing it restores the reading position. Pasted text does not execute reader shortcuts.
- Streaming restores the cursor and terminal settings after interruption or termination, including while waiting for upstream input.

Empty documents still exit immediately. Use `--ansi` to print once.

## 0.0.3

Read long files from their beginning with continuous mouse-wheel scrolling.

- Static files use the terminal's main screen, with wheel/arrows, beginning/end navigation and resize handling. q/Esc returns to the shell.
- Add `--reader inline|native` and optional configuration. No setup is needed by default; native Kaku positioning requires an adapter and falls back to inline reading when unavailable.

The default file behavior changed. Use `--ansi` for one-shot output; pipes and
redirection are unchanged. At this release, real Kaku display, copying and native
positioning still required acceptance.

## 0.0.2

Opt into more prominent top-level headings in Kaku.

- `--large-headings` doubles the width and height of top-level H1 in static Markdown files. Other headings and body text retain ordinary size.

Disabled by default and unavailable for streaming. Terminal copying of enlarged
headings can duplicate text.

## 0.0.1

Initial daily-use version for Markdown files and piped output.

- Render headings, emphasis, lists, blockquotes, links, highlighted code and Chinese/English tables measured by display width.
- Render incoming Markdown progressively, committing completed content and reflowing the active area on resize. Restore the cursor after completion and Ctrl-C.
- Include three built-in themes, automatic light/dark selection and custom TOML themes.

Kaku was the primary trial environment. Ghostty, kitty and live incremental Claude
pipelines had not completed acceptance. This version did not change heading font
size or provide a full-screen viewer, search or images.
