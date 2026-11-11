# Architecture

TMD converts Markdown to styled logical lines; CLI selects an output mode from
the input and terminal capabilities. Current releases read documents without
modifying them or changing terminal or shell configuration.

## Module boundaries

The Cargo workspace has two crates:

| Layer | Responsibility | Main modules |
| --- | --- | --- |
| `tmd-core` | Parsing, display width, wrapping, layout, themes, highlighting, stream commitment | `parse`, `html`, `width`, `wrap`, `layout`, `table`, `code`, `highlight`, `theme`, `theme_file`, `streamer` |
| `tmd-cli` | Arguments, input adapters, output selection, ANSI encoding, terminal lifecycle | `cli`, `source`, `claude`, `ansi`, `stream`, `reader`, `reader_native`, `large_headings` |

Core has no terminal dependencies and does not inspect environment variables.
CLI `main` assembles input, theme and output. `source::Source` exposes `push`,
`finish`, `set_width` and a diagnostic queue for Markdown and Claude JSONL.
Adapters never draw or print diagnostics directly.

Core retains image URLs, alt text and size hints; `Document::images` assigns
session indexes. Explicit image layout receives known pixel and cell dimensions
and emits ordinary logical lines with `ImageCell`. It performs no IO or graphics
protocol operations. CLI `image_loader` bounds loading/rasterization across two
workers; `reader_images` owns caches, chunked Kitty transfers and ID-scoped
cleanup. Plain and streaming rendering retain alt text. New image dimensions
invalidate both reader layout caches and restore a text/source anchor. Positions
inside an image preserve a row ratio. See [image usage](reader.md#images).

`config` resolves the configuration directory; `theme_select` loads themes;
`reader_config` validates reading settings. `terminal_io` shares cancellable
writes, reset sequences and direct-Kaku detection. Each output mode keeps its
own lifecycle management.

## Image resource limits

Load only the visible viewport and one adjacent viewport using at most two
workers. Evicted prefetches are not downloaded again until visible. Font/pixel
size changes invalidate image layout; successful preparation must not repeatedly
relayout the whole document.

The raster cache accounts for terminal decoding cost and is limited to 64 MiB,
1,024 resources and 256 prepared size variants. Each input is limited to 8 MiB
and raster decoding to 16 megapixels; output is downsampled to at most 2,048
pixels per side. HTTP requests have an eight-second timeout and at most three
redirects.

SVG input is limited to 2 MiB and 20,000 tags. Check nesting before recursive XML
parsing. XML and expanded-reference depth are limited to 32; reference expansion
to 20,000 nodes. Reject DTD, SVGZ, cyclic references, CSS escapes and stylesheet
URL references. Do not execute scripts or load external/embedded images. Failures
retain alt text; worker stack overflow must not abort the reader.

## Parsing and layout

`parse` turns pulldown-cmark events into blocks with source byte ranges. `html`
handles visible HTML text and structure; tags are never emitted as terminal
controls. Layout converts blocks to `Vec<Line>`.

- `Span` carries text and style. `Line` carries spans, a source range and `LineSize`.
- `Line::width()` counts terminal columns; `physical_height()` counts rows. Ordinary lines occupy one row; large lines double both dimensions.
- Wrapped lines in a block share its source range. Top-level code retains per-line ranges for streaming commitment. Inter-block blank lines belong to the following block.
- `width` owns cluster handling, measurement and truncation using Unicode `width()` semantics. Wrapping never splits recognized combining-character or emoji clusters.
- Tables are measured before allocating widths and drawing. Syntect highlights code. Its process-local locked cache reuses prefixes; a single large code block may exceed the target cached line count, so this is not a strict memory bound.

Built-in and user TOML themes use the same layout. See the
[theme schema](../themes/README.md) for colors, symbols and validation.

## Input and output selection

`tmd help` prints CLI help and exits before theme, terminal or input IO. The
legacy `--help` and `-h` flags remain supported.

| Condition | Output |
| --- | --- |
| No file, piped stdin, terminal stdout, no `--ansi` | Streaming |
| On Unix: Markdown file, terminal stdin/stdout, no `--ansi`, initial size at least 4 columns and 4 rows, `TERM` set and not `dumb` | Main-screen reader, optionally native Kaku integration |
| Otherwise | Read to completion, then print once |

`--from claude-json` handles incremental parent-message events and ignores
unknown events. It avoids repeating complete assistant messages. Tool starts and
results are separate output; scrollback is immutable. `--debug` diagnostics omit
raw records and pass through the output driver.

One-shot width is `--width`, otherwise terminal width, otherwise 80. Reader width
is bounded by the window. Invalid UTF-8 becomes replacement characters. Redirected
stdout has no OSC 8 links. A consumer closing a pipe early is normal termination.

## Streaming commitment

`Streamer` stores growing source and reparses/renders it; it is not an incremental
parser. Only complete source lines determine commitment. The last top-level
block stays active; complete code lines can commit early.

`Frame { committed, active }` separates lines written once from the active frame,
which is replaced in full. Resize changes only active width. CLI coalesces input
into synchronized frames roughly every 16 ms and bounds active height to the
viewport.

Streaming stays out of raw mode, so Ctrl-C acts on the shell pipeline. Diagnostics
clear and repaint the active area. INT, TERM, HUP and QUIT set a flag and wake the
input channel, so an idle wait blocks without polling and a signal never waits
behind input that was already read. The output thread restores cursor and any
changed Unicode version, then ends the process by the same signal, so a calling
shell stops its script or loop. QUIT exits with `128 + 3` instead, because its
default action dumps core and macOS records a crash report. Blocked writes must
become writable before cleanup can finish.
Check terminal capabilities before changing Unicode protocol settings.

## Static reader

`Document` parses and lays out the complete static file in memory. Its semantic
text index removes Markdown markers and separates blocks, table cells and code
lines. `Span.text_ranges` maps displayed text one way into that index; decorations
are excluded. Ordinary `render` and `Streamer` spans do not have this mapping.
`Line.src` identifies a source block, not an exact search range or an editing
writeback position.

The reader moves existing visible content into history, shows invocation context
at the top and starts at the document beginning. It never enters the alternate
screen or clears history. Nonempty documents stay open; empty documents exit.
Raw mode, mouse reporting and bracketed paste are temporary. Pasted text does
not execute reader shortcuts.

`Document.code_blocks()` retains original code text only for static reading.
Chooser previews sanitize controls and truncate; confirmed copying uses full
text. CLI owns clipboard IO, with a local macOS backend and test doubles.
`Span.code_block` associates code lines, labels and blank lines with chooser
entries, including nested code. Ordinary and streaming rendering omit it.

`Document.headings()` indexes top-level H1-H6 with level, semantic title, source
range and line number. Repeated and empty titles retain distinct identities.
`heading_line` maps ranges to layout lines, skipping preceding separators rather
than searching by title.

CLI `View` separates reading, query input, outline, code chooser and help with
its return origin. Sidebar visibility and keyboard focus are independent. At 80
columns, outline and ordinary-size text use separate column ranges; narrower
windows temporarily show a full-page outline without losing split-view focus.
Enter keeps sidebar focus after jumping, but closes a full-page list. Tab switches
panes; wheel routing uses pointer position without changing keyboard focus.
Clipping respects character clusters.

`reader_outline` keeps selection and current section separately, drawn as a theme
background and leading bar. `reader_ui` shares list viewports, width-aware footers
and timed feedback. Temporary views preserve document and list positions. Tiny
window states retain the original anchor for later expansion.

Cache at most full-width and sidebar layouts for one window size, moving ownership
rather than cloning the document. Resize invalidates them. The document, theme
and CLI options are immutable within a read-only session; caches are not shared
across sessions. New layout restores a semantic-text anchor, falling back to a
source block; large headings enter the viewport as complete pairs.

Query input displays a real cursor for IME positioning. Cancel restores the
reading position; confirmation locates the active match. Exit restores original
termios, mouse/paste modes, cursor, styles and DEC line attributes.

### Native Kaku integration

`contrib/kaku-reader.lua` is optional and direct-Kaku only. Its v1 preflight uses
a unique marker and acknowledgement file in a private mode-0700 temporary
directory. The adapter appends matching history capacity. The reader waits at
most 500 ms and removes the file; late replies never enter keyboard input.

Only print the full document when history capacity is sufficient and no input
is pending. Unsupported, timed-out or insufficient-capacity preflight falls back
to inline reading before output. Failure to position after output never prints
the document again. The adapter executes no shell commands and edits no user
configuration. Concurrent external output and actual event timing need real
terminal verification. See [setup](reader.md#optional-native-kaku-mode).

## Large headings and recovery

`--large-headings` checks static input, direct Kaku, TTY, dimensions and width
before passing `RenderOptions` to core. Only top-level H1 is enlarged; other
headings, body text and separators remain ordinary. Default rendering and
`Streamer::new` always use ordinary lines.

Reader and one-shot large-heading output translate DEC double-height lines into
upper/lower halves. SGR reset is insufficient; explicitly emit `ESC # 5`. Terminal
copying can duplicate text because both halves contain it.

One-shot large-heading output retains canonical mode and ISIG, temporarily enables
`NOFLSH`, and disables `ECHO`/`ECHONL` so VINTR cannot discard a half-line or inject
echoed controls. On failure, clean up any unmatched half before restoring termios.
This lifecycle differs from both raw reading and non-raw streaming; do not share
a session guard across them.

## Verification

Rust tests, snapshots, PTY protocol checks and real-terminal acceptance provide
different evidence. See [testing](terminal-testing.md) and [releases](release.md).
This document describes current implementation; plans and review records stay
outside the repository.
