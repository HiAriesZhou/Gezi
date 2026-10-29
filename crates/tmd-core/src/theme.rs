use crate::style::{Color, Style};

/// Styles and symbols for every rendered element.
///
/// Only the built-in default exists until T5 adds TOML themes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
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
}

impl Default for Theme {
    fn default() -> Self {
        let gray = Color::Ansi(8);
        Self {
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
            muted: Style::fg(gray),
            symbols: Symbols {
                bullets: ["•", "◦", "▪"].map(String::from).to_vec(),
                task_unchecked: "☐".into(),
                task_checked: "☑".into(),
                quote_bar: "│".into(),
                rule: "─".into(),
            },
        }
    }
}
