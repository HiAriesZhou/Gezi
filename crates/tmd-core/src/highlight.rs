//! Code highlighting with syntect's built-in syntaxes and themes, loaded
//! the first time a code block needs them.

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::style::{Color, Style};

const DARK_THEME: &str = "base16-ocean.dark";
const LIGHT_THEME: &str = "InspiredGitHub";

/// Fence names syntect's default set lacks or spells differently. It has
/// no TypeScript, so TypeScript borrows JavaScript, which colors everything
/// but the type annotations correctly.
const ALIASES: &[(&str, &str)] = &[
    ("typescript", "js"),
    ("ts", "js"),
    ("tsx", "js"),
    ("mts", "js"),
    ("cts", "js"),
    ("jsx", "js"),
    ("sh", "bash"),
    ("shell", "bash"),
    ("zsh", "bash"),
    ("console", "bash"),
];

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();

/// Styled pieces for each line of `lines`, or `None` when the language is
/// unknown. Styles carry only foreground color and font attributes; the
/// caller lays them over the code block background.
pub(crate) fn highlight(
    lang: &str,
    lines: &[&str],
    dark: bool,
) -> Option<Vec<Vec<(String, Style)>>> {
    let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let lang = lang.to_ascii_lowercase();
    let token = ALIASES
        .iter()
        .find(|(alias, _)| *alias == lang)
        .map_or(lang.as_str(), |(_, target)| target);
    let syntax = syntaxes
        .find_syntax_by_token(token)
        // Plain text has nothing to color; keep the terminal's foreground.
        .filter(|syntax| syntax.name != syntaxes.find_syntax_plain_text().name)?;
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    let theme = themes
        .themes
        .get(if dark { DARK_THEME } else { LIGHT_THEME })?;

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        // The newline-aware syntaxes need each line to end with `\n`.
        let source = format!("{line}\n");
        let pieces = highlighter.highlight_line(&source, syntaxes).ok()?;
        out.push(
            pieces
                .into_iter()
                .map(|(style, text)| (text.trim_end_matches('\n').to_string(), convert(style)))
                .filter(|(text, _)| !text.is_empty())
                .collect(),
        );
    }
    Some(out)
}

fn convert(style: syntect::highlighting::Style) -> Style {
    let fg = style.foreground;
    Style {
        fg: Color::Rgb(fg.r, fg.g, fg.b),
        bold: style.font_style.contains(FontStyle::BOLD),
        italic: style.font_style.contains(FontStyle::ITALIC),
        underline: style.font_style.contains(FontStyle::UNDERLINE),
        ..Style::default()
    }
}
