use std::sync::OnceLock;

use crate::style::Style;
use crate::theme_file::{self, ThemeError};

/// Styles and symbols for every rendered element. Built-in themes live in
/// `themes/*.toml`; field reference in `themes/README.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Whether the theme is meant for a dark background; picks the code
    /// highlighting colors and the base for partial theme files.
    pub dark: bool,
    pub h1: Style,
    pub h2: Style,
    /// Headings of level 3 and below.
    pub h3: Style,
    pub text: Style,
    pub bold: Style,
    pub italic: Style,
    pub strike: Style,
    pub inline_code: Style,
    /// Code block rows; its background fills the whole row.
    pub code_block: Style,
    pub link: Style,
    pub quote_bar: Style,
    pub list_marker: Style,
    pub rule: Style,
    pub table_border: Style,
    pub table_header: Style,
    /// Image placeholders.
    pub muted: Style,
    /// One-line summaries of folded content (Claude Code thinking and tool
    /// calls).
    pub fold: Style,
    pub symbols: Symbols,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbols {
    /// Bullet per nesting level, cycling when lists nest deeper.
    pub bullets: Vec<String>,
    pub task_unchecked: String,
    pub task_checked: String,
    pub quote_bar: String,
    /// Repeated to draw thematic breaks.
    pub rule: String,
    /// Repeated under level-1 and level-2 headings; empty for no line.
    pub h1_rule: String,
    pub h2_rule: String,
    pub table: TableSymbols,
}

/// Box-drawing characters for table borders; each one column wide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSymbols {
    pub top_left: String,
    pub top_mid: String,
    pub top_right: String,
    pub mid_left: String,
    pub mid_mid: String,
    pub mid_right: String,
    pub bottom_left: String,
    pub bottom_mid: String,
    pub bottom_right: String,
    pub vertical: String,
    pub horizontal: String,
}

/// Built-in themes: name and TOML source.
const BUILTIN: [(&str, &str); 3] = [
    (
        "default-dark",
        include_str!("../../../themes/default-dark.toml"),
    ),
    (
        "default-light",
        include_str!("../../../themes/default-light.toml"),
    ),
    (
        "neon-tokyo",
        include_str!("../../../themes/neon-tokyo.toml"),
    ),
];

impl Theme {
    /// Names of the built-in themes.
    pub fn builtin_names() -> impl Iterator<Item = &'static str> {
        BUILTIN.iter().map(|(name, _)| *name)
    }

    /// A built-in theme by name.
    pub fn builtin(name: &str) -> Option<Theme> {
        match name {
            "default-dark" => Some(base(true).clone()),
            "default-light" => Some(base(false).clone()),
            _ => BUILTIN
                .iter()
                .find(|(builtin, _)| *builtin == name)
                .map(|(_, source)| Theme::from_toml(source).expect("built-in themes are valid")),
        }
    }

    /// Parses a theme file. Fields it leaves out come from `default-dark`
    /// or `default-light`, whichever matches its `dark` setting (dark when
    /// unset).
    pub fn from_toml(source: &str) -> Result<Theme, ThemeError> {
        theme_file::parse(source, Some(base(theme_file::wants_dark(source))))
    }
}

/// The complete default theme for a dark or light background.
fn base(dark: bool) -> &'static Theme {
    static DARK: OnceLock<Theme> = OnceLock::new();
    static LIGHT: OnceLock<Theme> = OnceLock::new();
    let (cell, index) = if dark { (&DARK, 0) } else { (&LIGHT, 1) };
    cell.get_or_init(|| {
        theme_file::parse(BUILTIN[index].1, None).expect("built-in base themes are complete")
    })
}

impl Default for Theme {
    fn default() -> Self {
        base(true).clone()
    }
}
