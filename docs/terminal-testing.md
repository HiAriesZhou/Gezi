# Terminal testing

## Automated checks

Run formatting, Clippy and Rust tests, then build the candidate:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --locked
python3 scripts/check_stream_pty.py --binary target/release/gezi
python3 scripts/test_stream_pty.py --binary target/release/gezi
python3 -B scripts/test_reader_pty.py --binary target/release/gezi
python3 -B scripts/test_images_pty.py --binary target/release/gezi
python3 -B -m unittest discover -s scripts -p 'test_*release.py'
```

PTY checks use only Python's standard library and `/bin/bash` on macOS. They do
not load user shell configuration or save history. Pass `--binary <path>` to test
a downloaded package instead of a local build.

Image checks use synthetic SVG and a local HTTP server. They cover default
loading, remote/all-images off, responsiveness during slow requests, cropping,
sidebar/help transitions and session-owned cleanup. These verify protocol output,
not actual Kaku rendering.

Streaming probes cover split UTF-8, multiple frames, resize, normal completion and
INT/TERM/HUP termination by signal, QUIT exit status 131, restoration, and a shell loop stopping after
keyboard Ctrl-C. Scenarios use observed output rather
than fixed startup delays, for both Markdown and Claude JSONL. Failures return
nonzero. Setting `TERM_PROGRAM=Kaku` selects protocol branches; it is not real
Kaku acceptance.

CI runs the same checks on macOS. Installer tests use isolated destinations and
mock downloads to verify failed checksums, backups, repeat installation and
atomic replacement. After changing installation, also install an official Release
into a temporary directory; do not overwrite the personal installation.

Release candidates must pass [archive verification](release.md#building-a-candidate)
using the actual packaged executable, including version, help, rendering and PTY
behavior.

## Reader performance

Use a release build or an extracted download:

```sh
python3 -B scripts/benchmark_reader.py --binary target/release/gezi > /tmp/gezi-reader-performance.json
```

The isolated 80x24 PTY uses a 1 MiB synthetic Chinese/English document and 1,000
headings. It samples each action 100 times, including the first, and reports first,
P50, P95 and maximum latency for sidebar toggling, jumps, focus and search. P95
above 100 ms fails. Record hardware on an idle machine and keep results outside
the repository; shared CI contention is not a product performance measurement.

The reader reuses two layouts for a window size without copying the full document.
Initial sidebar opening and resizing still require layout. Timings end at the
synchronized-output end marker and exclude Kaku painting, IME and human input.

## Real-terminal checks

PTY success does not prove smooth painting, font alignment or terminal
compatibility. The current acceptance environment is Kaku on macOS:

```sh
python3 scripts/slowcat.py fixtures/long.md | target/release/gezi
target/release/gezi --width 80 fixtures/cjk-table.md
```

Check flicker, Chinese/emoji table alignment, progressive code output and active
height. Resize while streaming: only the active region should reflow. Repeat and
press Ctrl-C; verify upstream termination, cursor restoration and normal typing.
Record the actual terminal, executable version and result. Report automated
checks, visual acceptance and maintainer acceptance separately; mark environments
that were not exercised.

## Large headings

These noninteractive-stdin commands exercise one-shot output. Test the interactive
reader separately below. Use direct Kaku with at least 40 columns and 10 rows:

```sh
target/release/gezi --large-headings fixtures/large-headings.md < /dev/null
target/release/gezi fixtures/large-headings.md < /dev/null
target/release/gezi --large-headings --width 20 fixtures/large-headings.md < /dev/null
```

Keep screenshots and acceptance records outside the repository.

1. Only top-level ATX/Setext H1 doubles in width and height. H2-H6, nested H1, body, code and tables stay ordinary. H1 separators remain full-width and single-height.
2. Check Chinese, combining characters, emoji, styles and links. At 20 columns, wrapping must not split clusters or overlap. Upper/lower halves remain paired in scrollback.
3. After completion, run `printf 'normal prompt test\n'`. The prompt and later text must use ordinary size, including after resize. Static history does not promise reflow.
4. Copy a large heading into a plain-text editor and record duplication. Both physical halves contain the same text; this disclosed limitation is not lossless copying.
5. Repeat the sample in a temporary file outside the repository. Interrupt one-shot output with Ctrl-C; expect status 130, normal prompt size and no orphaned half-heading.
6. `--ansi`, redirected output, piped input and tmux/screen retain ordinary headings. Do not spoof terminal variables and call it real display acceptance.

Stream PTY tests cover static completion, interruption after observed heading
output, small windows and capability fallback. Rust tests inject errors or
cancellation at every output byte boundary. VINTR regressions write `0x03` to the
controlling terminal and verify temporary NOFLSH/no-echo, retained canonical/ISIG,
and restoration of original termios. HUP, QUIT and TERM are also covered. These
prove lifecycle/protocol behavior; Kaku visuals and copying need separate checks.

## Main-screen reader

Use the candidate binary for every scenario:

1. Launch `gezi fixtures/acceptance.md` from the top, middle and bottom of a window. Invocation context appears above the beginning of the document; no alternate screen is used.
2. Scroll continuously without loading more pages. Check Home/End, Shift-drag selection, persistent short documents and enlargement. Help can scroll and returns to the old position. Pasting text containing `q` does not exit.
3. With `--large-headings fixtures/large-headings.md`, scroll across viewport edges and resize. No half-heading remains and body text never becomes large.
4. Shrink to a narrow or one-row window, then expand. Anchors and exit remain usable. `q`/Esc restores the prompt; Ctrl-C returns 130. Run another command to check echo and inspect preserved shell history.
5. Search with the system Chinese IME. Candidate placement follows the input cursor; Enter confirms, counts and locates matches; n/N wraps. A typed q stays query text. Cancel after resize restores location. Check soft-wrapped words, no matches, help/search Esc behavior and Ctrl-C recovery.
6. Copy code with tabs, Chinese and long lines, then compare its text. Check CRLF normalization, nested code, trailing newline and empty-block clipboard preservation. Save personal clipboard content before manual tests; automated tests use fake backends. Cancel, scroll, search and exit must not alter it. Resize and interrupt the chooser to verify recovery.
7. Check configuration, CLI overrides and invalid settings. ANSI, redirection, Markdown pipes and Claude JSONL keep their behavior.
8. Use temporary Kaku configuration outside the repository for native integration; never overwrite personal settings. Successful preflight prints and positions at the beginning. Missing adapters or insufficient history fall back before output. Terminal scrolling/copying work and no extra input reaches the shell.
9. Open the outline with `o`. Include H1-H6, Setext, duplicate and empty headings; omit examples in quotes, lists and code. Moving selection leaves the document still. Repeat Down/Enter three times in a wide window without reopening or switching focus. Tab changes focus; Esc/q/o closes the focused outline. Selection background and current-section bar are independent; document scrolling updates only the latter. Inactive selection is muted. Check both themes, cluster-safe truncation and source lines for repeated titles.
10. At 80 columns the panes sit side by side; at 79 they become a full-page outline. Expansion restores selection and focus. Switching between short and long titles never inserts detail rows in the sidebar or changes page step. Wheel routing follows the pointer without changing focus. Opening the sidebar uses ordinary headings; closing restores requested large headings. Text never overwrites the separator.
11. Help in outline/code views preserves selection and list scroll on return. Footers show only valid actions at 80x24 and 40x12. Shrink to 24 columns and 4x1, then restore: destinations and search remain valid.
12. Empty-outline, jump and copy-success feedback disappears after three seconds; copy errors persist until another selection, retry or cancellation. Feedback never moves content or the IME cursor. On a 1,200-heading sample, navigate for 30 seconds and record stalls, artifacts or selection drift. PTY timing is not visual acceptance.
13. Run `gezi help` directly and with stdin held open. It prints CLI help and exits without entering the reader. `--help` and `-h` remain supported; a file named `help` can be opened as `./help` or after `--`.

Do not report a simulated native handshake as real adapter acceptance when
connection failed or the adapter was not installed.
