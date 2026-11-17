# Testing

## Automated checks

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 -B scripts/test_site.py
python3 -B -m unittest discover -s scripts -p 'test_*release.py'
```

For terminal changes, also exercise debug signal/lifecycle behavior:

```sh
cargo build --locked
python3 -B scripts/test_stream_pty.py --binary target/debug/gezi
```

Then [build and verify a release package](release.md#building-a-candidate). Its
checker runs the full terminal suite on the extracted executable:

| Script | Coverage |
| --- | --- |
| `test_reader_pty.py` | Navigation, search, copying controls, help, resize and reader lifecycle |
| `test_images_pty.py` | Image protocols, loading controls, cropping, caches and failure recovery |
| `check_stream_pty.py` | Markdown/Claude streams, resize, completion and keyboard interruption through a shell |
| `test_stream_pty.py` | Large-heading lifecycle, signals, startup and terminal restoration |

These scripts use Python's standard library and macOS `/bin/bash`, without user
shell configuration or history. For focused iteration, run a script directly
with `--binary <path>` instead of rebuilding and checking the whole package.

[CI](../.github/workflows/ci.yml) runs the complete package suite once, plus the
debug streaming checks. Each packaged suite streams diagnostics and reports its
time. Only PRs matching the workflow's explicit documentation allowlist skip the
Rust/package steps; every main push runs them.

### Stress and regression coverage

Ordinary Rust tests retain long-document line/mixed-byte boundaries, randomized
short fixtures and persisted failure seeds. Run the extra long-document random
small-chunk stress test when changing streaming commitment, parsing or highlighting:

```sh
cargo test -p tmd-core streaming_long_document_matches_a_full_render -- --ignored
```

The Rust event-loop test raises a signal after one frame while input, EOF and a
wake remain queued. PTY tests exercise actual signal delivery and restoration;
they do not prove instantaneous interruption of an arbitrarily large frame
already being written. INT/TERM/HUP terminate by signal; QUIT returns 131.

Large-heading interruption tests must send the terminal's actual VINTR character,
not only SIGINT. They check NOFLSH/no-echo, retained canonical mode and ISIG, DEC
line-size reset and termios restoration. Rust tests also inject failures and
cancellation at output byte boundaries.

### Installation and update checks

Tests use temporary installations and local download stubs for checksums, command
ownership, backups, rollback, signals and both executable aliases. Release tests
simulate GitHub responses; they create no remote releases or tags.

After changing installation, also check an official Release in a temporary
directory. Never update a personal installation for a test. Use a build at least
as new as the latest public release when checking live latest-version lookup.

## Reader performance

Use a release build or an extracted package on an idle machine:

```sh
python3 -B scripts/benchmark_reader.py --binary target/release/gezi > /tmp/gezi-reader-performance.json
```

The 80×24 pseudo-terminal uses a 1 MiB Chinese/English document with 1,000 headings
and 100 samples per action. It reports first-use, median, 95th-percentile and
maximum latency; a 95th percentile above 100 ms fails. Record hardware with the
results outside the repository. Timing ends at synchronized-output completion
and excludes Kaku painting, input methods and human input. Shared CI contention
is not a product performance measurement.

## Real-terminal acceptance

Protocol tests do not prove visual rendering or clipboard compatibility. Use the
candidate in direct Kaku on macOS, record the binary version and terminal, and
report automated results, visual checks and maintainer acceptance separately.
Do not call a simulated handshake or `TERM_PROGRAM=Kaku` real Kaku acceptance.

Use the [reader guide](reader.md) as the behavior reference. Exercise these
boundaries rather than repeating its entire control reference:

| Area | Acceptance checks |
| --- | --- |
| Lifecycle | Launch from different window positions; start at the document beginning without clearing history or using the alternate screen. Nonempty short files stay open. Test q/Esc, Ctrl-C and normal typing after exit. Pasted q must not exit. |
| Navigation and outline | Repeated jumps, duplicate/empty H1 through H6 titles, long titles and no headings. Test 79/80-column layout transitions, pointer-based wheel routing, independent selection/current-section markers and focus preservation. On a 1,200-heading sample, navigate for 30 seconds and watch for stalls or drift. |
| Search | Use a Chinese input method; check candidate position, live counts, current/all-match contrast, same-line repetitions, cross-style and wrapped text. Try no matches, literal q, n/N wrapping, cancellation after resize, and search/outline/help transitions. |
| Help and feedback | Check both languages and light/dark themes at 80×24, 40×12, 24-column and 4×1 sizes. Verify scrolling, return position, valid shortcuts and three-second feedback without content/cursor movement. |
| Images | Use synthetic local SVG and disposable remote URLs. Check crop/placement through scroll, resize, help, outline and search; loading controls; text alignment; and cleanup that leaves unrelated terminal images alone. |
| Configuration | Check command-line overrides, invalid settings, themes, language switching and native fallback. Plain output, pipes and Claude JSONL must retain their documented behavior. |

### Clipboard

Save personal clipboard content before manual testing and use disposable text.
Drag without modifiers in both directions across visible lines, with Chinese,
combining marks, emoji, tables, large H1 rows and the outline open/closed. The blue
selection must match the copied display text. Click, cancel, scroll or resize
must not copy; keyboard and wheel navigation must remain usable.

Click Copy on code with tabs, trailing spaces, Chinese, long lines, CRLF, nested
blocks and empty content. Compare with the original snippet; check in-place
success/error feedback, narrow blocks and outline offsets. A resize between
press and release cancels the click. Copy buttons must not start drag selection;
selection across a header must exclude button labels. Pressing `c` must not open
a chooser. Automated tests use fake clipboard backends or cancel before release.

### Streaming and large headings

```sh
python3 scripts/slowcat.py fixtures/long.md | target/release/gezi
target/release/gezi --width 80 fixtures/cjk-table.md
target/release/gezi --large-headings fixtures/large-headings.md < /dev/null
target/release/gezi --large-headings --width 20 fixtures/large-headings.md < /dev/null
```

Check progressive output, flicker, Chinese/emoji alignment and resize: only the
active stream area reflows. Ctrl-C must stop the upstream pipeline and restore
normal input. The redirected-stdin commands exercise one-shot output; also open
the same file interactively and scroll across viewport edges.

Only top-level H1 should enlarge. Check complete upper/lower pairs, ordinary body
text, small-window/capability fallback and `printf 'normal prompt test\n'` after
exit. Interrupt a temporary sample and verify status 130 with no orphaned half.
Copy a large heading into a text editor to check the documented duplicate-half
limitation. `--ansi`, pipes and tmux/screen retain ordinary headings.

### Native Kaku integration

Use temporary configuration outside the repository; preserve personal settings.
Follow [native setup](reader.md#optional-native-kaku-mode). Check successful
positioning at the beginning, terminal scrolling/selection, and fallback before
output when the adapter is missing or history is insufficient. No extra input
should reach the shell. If the adapter cannot connect, report that gap rather
than claiming compatibility. Keep screenshots and acceptance records outside
the repository.

### Mermaid previews

Run the candidate with `fixtures/mermaid.md` in direct Kaku. Check all six basic
types in light and dark themes, especially Chinese labels, line intersections
and text clipping. Use `m` on each block, search for a source-only identifier,
clear the search, and check that Copy retains the full source. Repeat with a
narrow column, the outline, scrolling and resize. Confirm the unsupported pie
block keeps its source. Check `--images off`, `--remote-images off`, redirected
output and streams. Exit during loading and confirm normal terminal input and
no surviving Mermaid worker or session temporary directory. Protocol tests do
not establish visual readability or maintainer acceptance.
