# Themes

Themes are TOML files. Three palettes are built in:

| Name | Palette |
| --- | --- |
| `default-dark` | Dark background; ANSI 16 colors follow terminal settings |
| `default-light` | Light background; ANSI 16 colors |
| `neon-tokyo` | Dark background; true color |

`--theme auto` is the default. It queries the terminal background for up to 300 ms,
then falls back to `COLORFGBG`, then dark. Non-terminal output uses dark directly.

## Custom themes

Save a file to `~/.config/gezi/themes/<name>.toml`, or
`$XDG_CONFIG_HOME/gezi/themes/<name>.toml` when that variable is set. Select it with
`gezi --theme <name>`. Names accept letters, digits, `-` and `_`. A user file with
a built-in name overrides that palette. If a file is absent in `gezi/themes`,
Gezi also checks the old `tmd/themes` location. An invalid or unreadable Gezi file
is reported as an error rather than replaced by a legacy theme.

Every field is optional. Missing fields use `default-dark`, or `default-light`
when `dark = false`. A supplied style replaces the entire default style; its
individual fields are not merged.

For example, change only the H1 style:

```toml
[styles]
h1 = { fg = "#ff007c", bold = true }
```

Errors identify the file, line and cause:

```text
gezi: theme file ~/.config/gezi/themes/mine.toml: line 3: invalid color "blu": ...
```

## Colors

| Value | Meaning |
| --- | --- |
| `"ansi:0"` to `"ansi:7"` | Terminal colors: black, red, green, yellow, blue, magenta, cyan, white |
| `"bright:0"` to `"bright:7"` | Bright variants; `bright:0` is usually gray |
| `"#RRGGBB"` | True color, for example `"#4DF2FF"` |
| `"default"` | Terminal foreground or background |

ANSI colors follow the terminal palette. True colors stay fixed and require
true-color terminal support.

## Top-level fields

| Field | Type | Meaning |
| --- | --- | --- |
| `dark` | Boolean | Whether the background is dark; defaults to `true`. Selects code highlighting (`base16-ocean.dark` or `InspiredGitHub`) and the base palette for missing fields. |

## `[styles]`

Each style is a table. All fields are optional:

| Field | Type | Meaning |
| --- | --- | --- |
| `fg`, `bg` | Color | Foreground/background; omitted values use terminal defaults |
| `bold`, `italic`, `underline`, `strike`, `dim` | Boolean | Text attributes; omitted values are `false` |

| Style | Used for |
| --- | --- |
| `h1` | H1 text and its separator |
| `h2` | H2 text |
| `h3` | H3-H6 text |
| `text` | Body text |
| `bold` | Strong emphasis; color helps fonts without Chinese bold glyphs |
| `italic` | Emphasis; color helps fonts without Chinese italic glyphs |
| `strike` | Strikethrough |
| `inline_code` | Inline code |
| `code_block` | Full-row code background; syntax highlighting supplies foreground colors, otherwise `fg` is used |
| `link` | Link text |
| `quote_bar` | Leading blockquote bar |
| `list_marker` | Bullets and ordered-list numbers |
| `rule` | Rules and H2 separators |
| `table_border` | Table borders |
| `table_header` | Table headers |
| `muted` | Textual image placeholders |
| `fold` | Single-line folded reasoning/tool entries in Claude event streams |

## `[symbols]`

Symbols cannot contain controls such as ESC or newlines. They are written to the
terminal, so invalid control characters are rejected.

| Field | Type | Meaning |
| --- | --- | --- |
| `bullets` | String array | Nested unordered-list markers; at least one; cycles at deeper levels |
| `task_unchecked` | String | Unchecked task marker, default `☐` |
| `task_checked` | String | Checked task marker, default `☑` |
| `quote_bar` | String | Blockquote bar, default `│` |
| `rule` | String | Repeated rule symbol, default `─` |
| `h1_rule` | String | H1 separator, default `━`; empty disables it |
| `h2_rule` | String | H2 separator, default `─`; empty disables it |

## `[symbols.table]`

Every table-border character must occupy exactly one column.

| Fields | Defaults | Position |
| --- | --- | --- |
| `top_left`, `top_mid`, `top_right` | `┌ ┬ ┐` | Top edge |
| `mid_left`, `mid_mid`, `mid_right` | `├ ┼ ┤` | Below the header |
| `bottom_left`, `bottom_mid`, `bottom_right` | `└ ┴ ┘` | Bottom edge |
| `vertical` | `│` | Vertical edge |
| `horizontal` | `─` | Horizontal edge |

Complete examples: [default-dark.toml](default-dark.toml),
[default-light.toml](default-light.toml), [neon-tokyo.toml](neon-tokyo.toml).
