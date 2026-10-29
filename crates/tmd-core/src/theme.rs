use crate::style::{Color, Style};

/// Styles and symbols for every rendered element.
///
/// Only the built-in default exists until T5 adds TOML themes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Whether the theme is meant for a dark background; picks the code
    /// highlighting colors.
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
    /// Raw HTML and image placeholders.
    pub muted: Style,
    pub symbols: Symbols,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbols {
    /// Bullet per nesting level, cycling when lists nest deeper.
    pub bullets: Vec<String>,
    pub task_unchecked: String,
    pub task_checked: String,
    pub quote_bar: String,
    /// Repeated to draw rules and the line under level-2 headings.
    pub rule: String,
    pub table: TableSymbols,
}

/// Box-drawing characters for table borders.
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

impl Default for Theme {
    fn default() -> Self {
        let gray = Color::Ansi(8);
        Self {
            dark: true,
            h1: Style::fg(Color::Ansi(13)).bold(),
            h2: Style::fg(Color::Ansi(12)).bold(),
            h3: Style::fg(Color::Ansi(14)),
            text: Style::default(),
            bold: Style::default().bold(),
            italic: Style::default().italic(),
            strike: Style::default().strike(),
            inline_code: Style::fg(Color::Ansi(11)).on(gray),
            code_block: Style::default().on(gray),
            link: Style::fg(Color::Ansi(12)).underline(),
            quote_bar: Style::fg(gray),
            list_marker: Style::fg(Color::Ansi(14)),
            rule: Style::fg(gray),
            table_border: Style::fg(gray),
            table_header: Style::default().bold(),
            muted: Style::fg(gray),
            symbols: Symbols {
                bullets: ["•", "◦", "▪"].map(String::from).to_vec(),
                task_unchecked: "☐".into(),
                task_checked: "☑".into(),
                quote_bar: "│".into(),
                rule: "─".into(),
                table: TableSymbols {
                    top_left: "┌".into(),
                    top_mid: "┬".into(),
                    top_right: "┐".into(),
                    mid_left: "├".into(),
                    mid_mid: "┼".into(),
                    mid_right: "┤".into(),
                    bottom_left: "└".into(),
                    bottom_mid: "┴".into(),
                    bottom_right: "┘".into(),
                    vertical: "│".into(),
                    horizontal: "─".into(),
                },
            },
        }
    }
}
