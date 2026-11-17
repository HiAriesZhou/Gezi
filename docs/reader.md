# Reader guide

`gezi article.md` opens the built-in reader when stdin and stdout are terminals.
It starts at the beginning, uses the main screen and preserves terminal history.
Files are currently read-only. Nonempty documents stay open until you exit.
`ge` is an equivalent short command. Run `gezi help` for command-line options (`--help` and `-h` also work).
To read a file named `help` or `update`, use an explicit path or `--`, such as
`gezi ./update` or `gezi -- update`; otherwise these names run CLI commands.

## Navigation

| Control | Action |
| --- | --- |
| Wheel, ↑ / ↓, `j` / `k` | Scroll |
| Home / End | Beginning / end |
| PageUp / PageDown, `b` / Space | Previous / next page |
| `/`, Enter, `n` / `N` | Search, confirm, next / previous match |
| `o`, Tab | Toggle the outline, switch sidebar/document focus |
| Click `Copy` | Copy the whole code block without leaving the document |
| `m` | Toggle the first visible Mermaid block between native preview and source |
| `L` / F2 | Switch English / Chinese for this session; use F2 while typing a search |
| `?` | Help for the current view |
| `q` | Exit reading; close a focused list or help |
| Esc | Cancel input or close a list/help; while reading, clear search, then close the sidebar, then exit |
| Ctrl-C | Exit from any view with status 130 |

The footer shows shortcuts for the focused view; it is not clickable. Resizing
preserves a text/source anchor, not an exact screen row.

## Help

Press `?` for grouped shortcuts for the current view. Scroll with the wheel or
navigation keys; `?`, Esc or `q` returns to the previous view without losing its
position. Narrow windows place descriptions below their shortcut labels.

## Interface language

The interface supports English and Simplified Chinese. Automatic selection uses:

1. In Kaku (`TERM_PROGRAM=Kaku`), macOS's app-specific `AppleLanguages` preference
   for `fun.tw93.kaku`, if configured.
2. The macOS preferred language list.
3. The first nonempty `LC_ALL`, `LC_MESSAGES` or `LANG` locale, if macOS preferences
   provide neither supported language.
4. English.

Each list uses its first supported English or Chinese entry; Chinese variants
use Simplified Chinese. Gezi reads macOS preferences without changing them or
executing Kaku's Lua configuration.

Use `gezi --language en article.md` or `gezi --language zh article.md` to override
automatic selection; `--language auto` restores it for a new process. Press `L`
or F2 to switch immediately in the reader. While entering a search, `L` remains
query text and F2 switches language. Switching preserves the reading position,
query and outline state; the choice lasts only for that process.

Reader controls, help, copy labels and Claude stream status use this language.
Document text, paths, code and tool output stay unchanged. Parser, updater and
system diagnostics retain their original language; core image fallback labels
still use Chinese (`图片` / `无法加载`).

## Search

Search is literal and case-sensitive, including Chinese and emoji. It covers
rendered text, headings and code, including offscreen content. Hidden link URLs,
decorations, regular expressions and cross-paragraph matches are excluded.
Typing previews matches immediately and moves to the first result. All visible
matches have a yellow background; the current match uses cyan, bold and underline.
Highlights follow text across styles and wrapped lines without changing layout.
Enter keeps the preview; `n` / `N` wrap around the results. An empty query keeps
the previous search; a query with no matches keeps the reading position.

While entering a query, letters are text. Esc restores the previous search,
selected match and reading position; Ctrl-U clears the input. Arrow keys,
Home/End and Backspace/Delete edit it. The visible cursor
anchors the system input method. Narrow tables may show a match at an ellipsis;
widen the window to see the text.

## Copying

Drag with the left mouse button to select visible text in either direction;
release to copy. No modifier is needed. Selection uses a blue background and
handles Chinese, combining marks and emoji. It copies displayed text and line
breaks, including decorations but excluding Copy controls and trailing display
spaces. Dragging outside the document stops at its visible edge.

A click without a drag does not copy. Keyboard input, scrolling and resize cancel
selection. The reader implements selection itself so the outline and wheel remain
usable without changing Kaku configuration.

For original code, click **Copy** at the block's top right (**复制** in Chinese).
This preserves tabs, indentation, trailing spaces and line breaks, with CRLF
normalized to LF. It excludes fences, wrapping and borders. Empty blocks leave
the clipboard unchanged. Success or retry feedback appears in place; no chooser
opens and the document stays put. Very narrow blocks use compact labels; blocks
under four columns have no button.

Press and release on the same button. Dragging, scrolling, resizing or a keyboard
action cancels the click. Both copy methods require local macOS, not SSH, and are
available only in the interactive inline reader.

## Heading outline

Press `o` to open the outline at your current section. Windows at least 80 columns
wide show a left sidebar; narrower windows show a full-page list.

- Move the selection with arrows, `j` / `k`, paging keys or Home/End. This does not move the document.
- In the sidebar, Enter jumps and keeps focus so you can jump again. Tab switches panes.
- In the full-page list, Enter jumps and returns to the document.
- The selection has a background; a leading `▎` marks the current section. The two can differ. Inactive selection is muted.
- The wheel scrolls the pane under the pointer without changing focus. Esc, `q` or `o` closes a focused outline.

Headings retain their source order and level. Repeated and empty headings have
separate destinations. Long titles are truncated; the header shows the source
line. Search and help preserve the sidebar when returning; clicking Copy keeps it open.
Split views use ordinary headings, even with `--large-headings`.

## Images

The inline reader displays PNG, JPEG and static SVG in direct Kaku sessions with
Kitty graphics enabled. Markdown images and HTML `<img>` work; `<picture>` uses
its `<img>` fallback. Relative paths resolve from the Markdown file's directory,
not the shell's working directory. Formats are detected from content, so an
extension is not required.

```sh
gezi article.md                         # Local and remote images
gezi --remote-images off article.md     # Local only
gezi --images off article.md            # No image IO or graphics
```

Remote HTTP(S) images load in the background and contact the document's image
hosts. A proxy is optional: existing `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` and
`NO_PROXY` settings are respected, including lowercase forms. `ALL_PROXY` takes
precedence. gezi does not change proxy configuration.

Images keep their proportions and fit the document pane. Adjacent badges share
rows when they fit. Scrolling and resizing crop placements to the document;
failed or oversized images retain alt text and a failure label. Caches last only
for the current process. See [resource limits](architecture.md#image-resource-limits).

Animation, browser CSS, table-cell graphics, `file:`/`data:` URLs and images in
streaming, native or redirected output are not supported. Those output modes use
text labels. If Kitty graphics is disabled in Kaku, use `--images off`.

## Mermaid

Fenced blocks labelled `mermaid` show native previews in the inline reader in
direct Kaku. No Node.js, browser or network service is required. Basic flowcharts,
sequence diagrams, state diagrams, class diagrams, entity relationship diagrams
and mindmaps are supported. Other diagram types and detected rendering failures
show the complete source with an unavailable label.

This is a native interpretation, not the official Mermaid renderer. Layout and
syntax support differ; some unsupported statements may be ignored by the parser.
Use `m` to switch the first visible Mermaid block between preview and source.
The Copy button always copies the complete original code. Searching shows source
for every Mermaid block while the query is active; clearing the search restores
your previous preview/source choices. Preview labels remain in English.

Diagrams fit the text column and preserve their proportions. Dense or wide
diagrams can become too small to read; switch to source or widen the column.
Light/dark previews follow the document theme, with system CJK font fallbacks.
The preview does not reproduce every custom theme color.

`--images off` disables previews. `--remote-images off` does not disable them.
Native output, one-shot output, streams and other terminals retain source.
Limits include 64 KiB of source, 200 nodes, 400 edges, 200 combined annotations,
a 15-second worker deadline and 2 MiB of generated SVG. Exceeding a limit
keeps source visible. See [worker boundaries](architecture.md#mermaid-previews).

## Output and configuration

`--ansi` prints once and returns to the shell. Piped input streams incrementally;
completed output is not redrawn on resize. `--large-headings` is an experimental
option for double-size top-level H1 in static files in direct Kaku. Copying such
headings through the terminal can duplicate text.

The default theme is `auto`. Built-in palettes and custom TOML files are described
in [Themes](../themes/README.md).

No configuration is needed. The inline reader defaults to a content width of at
most 100 columns, centered within the document pane, including when the outline
is open. It reserves at least two margin columns per side when space permits;
tiny windows reduce the margin to keep content visible. Use
`gezi --width 120 article.md` for a wider column, or choose a smaller width. The
window still limits the result. `--width` does not change the terminal font.
Images share the text column's left edge. One-shot and streaming output retain
their existing width selection.

To select a reading mode, use `--reader inline|native`
or set `reader.mode` in `~/.config/gezi/config.toml` (under `$XDG_CONFIG_HOME` when
set). The command-line option wins; the default is `inline`. If the Gezi file is
absent, the legacy `tmd/config.toml` is read instead. Non-reader output ignores it.

### Optional native Kaku mode

Native mode prints the whole document, scrolls back to its beginning and exits.
The terminal handles scrolling and selection; interactive search, outline, code
copying and images are available in inline mode instead.

Existing TMD adapters remain compatible; do not register a second adapter if one
is already installed. The `TMD_READER` variable and `tmd-reader-v1` protocol
identifiers are retained for that compatibility.

1. Copy [kaku-reader.lua](../contrib/kaku-reader.lua) to `~/.config/kaku/gezi-reader.lua`; compare before replacing an existing file.
2. Add this line before `return config` in your Kaku configuration, preserving existing handlers:

   ```lua
   dofile(require('wezterm').config_dir .. '/gezi-reader.lua').setup()
   ```

3. Reload Kaku and run `gezi --reader native article.md`.

Native mode requires direct Kaku and enough terminal history. If the adapter is
unavailable, gezi falls back to inline mode before printing. To undo the setup,
select inline mode and remove the registration line. The adapter never edits
shell or terminal configuration itself.
