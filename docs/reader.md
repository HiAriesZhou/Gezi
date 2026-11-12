# Reader guide

`gezi article.md` opens the built-in reader when stdin and stdout are terminals.
It starts at the beginning, uses the main screen and preserves terminal history.
Files are currently read-only. Nonempty documents stay open until you exit.
`ge` is an equivalent short command. Run `gezi help` for command-line options (`--help` and `-h` also work).
To read a file named `help`, use `gezi ./help` or `gezi -- help`.

## Navigation

| Control | Action |
| --- | --- |
| Wheel, ↑ / ↓, `j` / `k` | Scroll |
| Home / End | Beginning / end |
| PageUp / PageDown, `b` / Space | Previous / next page |
| `/`, Enter, `n` / `N` | Search, confirm, next / previous match |
| `o`, Tab | Toggle the outline, switch sidebar/document focus |
| `c` | Choose a code block to copy |
| `?` | Help for the current view |
| `q` | Exit reading; close a focused list or help |
| Esc | Cancel input or close a list/help; while reading, clear search, then close the sidebar, then exit |
| Ctrl-C | Exit from any view with status 130 |

Kaku normally lets you hold Shift while dragging to select text. Custom terminal
bindings can change this. The footer shows shortcuts for the focused view; it is
not clickable. Resizing preserves a text/source anchor, not an exact screen row.

## Search and code copying

Search is literal and case-sensitive, including Chinese and emoji. It covers
rendered text, headings and code, including offscreen content. Hidden link URLs,
decorations, regular expressions and cross-paragraph matches are excluded.
`n` / `N` wrap around the results. An empty query keeps the previous search;
a query with no matches keeps the reading position.

While entering a query, letters are text, Esc cancels, and Ctrl-U clears the
input. Arrow keys, Home/End and Backspace/Delete edit it. The visible cursor
anchors the system input method. Narrow tables may show a match at an ellipsis;
widen the window to see the text.

Press `c`, select a code block and press Enter to copy. This preserves tabs,
indentation, trailing spaces and line breaks, with CRLF normalized to LF. Markdown
fences and rendered borders are excluded. Empty blocks leave the clipboard
unchanged. Esc, `q` or `c` cancels. Clipboard writes occur only on confirmation,
on local macOS; SSH copying is not supported.

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
line. Search, code selection and help preserve the sidebar when returning.
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

## Output and configuration

`--ansi` prints once and returns to the shell. Piped input streams incrementally;
completed output is not redrawn on resize. `--large-headings` is an experimental
option for double-size top-level H1 in static files in direct Kaku. Copying such
headings through the terminal can duplicate text.

The default theme is `auto`. Built-in palettes and custom TOML files are described
in [Themes](../themes/README.md).

No configuration is needed. To select a reading mode, use `--reader inline|native`
or set `reader.mode` in `~/.config/gezi/config.toml` (under `$XDG_CONFIG_HOME` when
set). The command-line option wins. Non-reader output ignores this file.

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
