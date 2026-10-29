//! Theme files: TOML with optional fields. Fields left out come from the
//! built-in default theme for the file's background (`dark`).

use serde::Deserialize;
use toml::Spanned;

use crate::style::{Color, Style};
use crate::theme::{Symbols, TableSymbols, Theme};
use crate::width::text_width;

/// A theme file that failed to parse or validate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeError {
    /// 1-based line of the problem, when known.
    pub line: Option<usize>,
    pub message: String,
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ThemeError {}

/// Parses `source`. With `base`, missing fields are copied from it;
/// without, every field is required (built-in base themes).
pub(crate) fn parse(source: &str, base: Option<&Theme>) -> Result<Theme, ThemeError> {
    let file: ThemeFile = toml::from_str(source).map_err(|err| ThemeError {
        line: err.span().map(|span| line_of(source, span.start)),
        message: err.message().trim().to_string(),
    })?;
    validate(&file.symbols).map_err(|invalid| ThemeError {
        line: Some(line_of(source, invalid.span.start)),
        message: format!("{}: {}", invalid.path, invalid.message),
    })?;
    resolve(file, base).map_err(|missing| ThemeError {
        line: None,
        message: format!("{}: missing", missing.path),
    })
}

/// The theme `dark` selects as the base for a file that sets `dark`.
pub(crate) fn wants_dark(source: &str) -> bool {
    #[derive(Deserialize)]
    struct Probe {
        dark: Option<bool>,
    }
    toml::from_str::<Probe>(source)
        .ok()
        .and_then(|probe| probe.dark)
        .unwrap_or(true)
}

fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].matches('\n').count() + 1
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    dark: Option<bool>,
    #[serde(default)]
    styles: StylesFile,
    #[serde(default)]
    symbols: SymbolsFile,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct StylesFile {
    h1: Option<StyleFile>,
    h2: Option<StyleFile>,
    h3: Option<StyleFile>,
    text: Option<StyleFile>,
    bold: Option<StyleFile>,
    italic: Option<StyleFile>,
    strike: Option<StyleFile>,
    inline_code: Option<StyleFile>,
    code_block: Option<StyleFile>,
    link: Option<StyleFile>,
    quote_bar: Option<StyleFile>,
    list_marker: Option<StyleFile>,
    rule: Option<StyleFile>,
    table_border: Option<StyleFile>,
    table_header: Option<StyleFile>,
    muted: Option<StyleFile>,
    fold: Option<StyleFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleFile {
    fg: Option<ColorValue>,
    bg: Option<ColorValue>,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    italic: bool,
    #[serde(default)]
    underline: bool,
    #[serde(default)]
    strike: bool,
    #[serde(default)]
    dim: bool,
}

impl From<StyleFile> for Style {
    fn from(file: StyleFile) -> Self {
        Style {
            fg: file.fg.map_or(Color::Reset, |c| c.0),
            bg: file.bg.map_or(Color::Reset, |c| c.0),
            bold: file.bold,
            italic: file.italic,
            underline: file.underline,
            strike: file.strike,
            dim: file.dim,
            link: None,
        }
    }
}

/// A color written as `ansi:N` or `bright:N` (N = 0–7), `#RRGGBB`, or
/// `default` for the terminal's own color.
struct ColorValue(Color);

impl<'de> Deserialize<'de> for ColorValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_color(&text).map(ColorValue).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "invalid color {text:?}: use \"ansi:0\"–\"ansi:7\", \"bright:0\"–\"bright:7\", \"#RRGGBB\" or \"default\""
            ))
        })
    }
}

pub(crate) fn parse_color(text: &str) -> Option<Color> {
    if text == "default" {
        return Some(Color::Reset);
    }
    if let Some(hex) = text.strip_prefix('#') {
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?));
    }
    let (kind, index) = text.split_once(':')?;
    let index: u8 = index.parse().ok().filter(|n| *n < 8)?;
    match kind {
        "ansi" => Some(Color::Ansi(index)),
        "bright" => Some(Color::Ansi(index + 8)),
        _ => None,
    }
}

/// Symbols keep their position in the file so validation errors can name
/// the line.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SymbolsFile {
    bullets: Option<Spanned<Vec<String>>>,
    task_unchecked: Option<Spanned<String>>,
    task_checked: Option<Spanned<String>>,
    quote_bar: Option<Spanned<String>>,
    rule: Option<Spanned<String>>,
    h1_rule: Option<Spanned<String>>,
    h2_rule: Option<Spanned<String>>,
    #[serde(default)]
    table: TableFile,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TableFile {
    top_left: Option<Spanned<String>>,
    top_mid: Option<Spanned<String>>,
    top_right: Option<Spanned<String>>,
    mid_left: Option<Spanned<String>>,
    mid_mid: Option<Spanned<String>>,
    mid_right: Option<Spanned<String>>,
    bottom_left: Option<Spanned<String>>,
    bottom_mid: Option<Spanned<String>>,
    bottom_right: Option<Spanned<String>>,
    vertical: Option<Spanned<String>>,
    horizontal: Option<Spanned<String>>,
}

/// A required field missing from a theme with no base to copy from.
struct Missing {
    path: &'static str,
}

fn pick<T>(value: Option<T>, base: Option<T>, path: &'static str) -> Result<T, Missing> {
    value.or(base).ok_or(Missing { path })
}

macro_rules! style {
    ($file:expr, $base:expr, $field:ident) => {
        pick(
            $file.$field.map(Style::from),
            $base.map(|b| b.$field.clone()),
            concat!("styles.", stringify!($field)),
        )?
    };
}

macro_rules! symbol {
    ($file:expr, $base:expr, $field:ident, $path:expr) => {
        pick(
            $file.$field.map(Spanned::into_inner),
            $base.map(|b| b.$field.clone()),
            $path,
        )?
    };
}

fn resolve(file: ThemeFile, base: Option<&Theme>) -> Result<Theme, Missing> {
    let s = file.styles;
    let theme = Theme {
        dark: pick(file.dark, base.map(|b| b.dark), "dark")?,
        h1: style!(s, base, h1),
        h2: style!(s, base, h2),
        h3: style!(s, base, h3),
        text: style!(s, base, text),
        bold: style!(s, base, bold),
        italic: style!(s, base, italic),
        strike: style!(s, base, strike),
        inline_code: style!(s, base, inline_code),
        code_block: style!(s, base, code_block),
        link: style!(s, base, link),
        quote_bar: style!(s, base, quote_bar),
        list_marker: style!(s, base, list_marker),
        rule: style!(s, base, rule),
        table_border: style!(s, base, table_border),
        table_header: style!(s, base, table_header),
        muted: style!(s, base, muted),
        fold: style!(s, base, fold),
        symbols: resolve_symbols(file.symbols, base.map(|b| &b.symbols))?,
    };
    Ok(theme)
}

fn resolve_symbols(file: SymbolsFile, base: Option<&Symbols>) -> Result<Symbols, Missing> {
    let t = file.table;
    let bt = base.map(|b| &b.table);
    Ok(Symbols {
        bullets: symbol!(file, base, bullets, "symbols.bullets"),
        task_unchecked: symbol!(file, base, task_unchecked, "symbols.task_unchecked"),
        task_checked: symbol!(file, base, task_checked, "symbols.task_checked"),
        quote_bar: symbol!(file, base, quote_bar, "symbols.quote_bar"),
        rule: symbol!(file, base, rule, "symbols.rule"),
        h1_rule: symbol!(file, base, h1_rule, "symbols.h1_rule"),
        h2_rule: symbol!(file, base, h2_rule, "symbols.h2_rule"),
        table: TableSymbols {
            top_left: symbol!(t, bt, top_left, "symbols.table.top_left"),
            top_mid: symbol!(t, bt, top_mid, "symbols.table.top_mid"),
            top_right: symbol!(t, bt, top_right, "symbols.table.top_right"),
            mid_left: symbol!(t, bt, mid_left, "symbols.table.mid_left"),
            mid_mid: symbol!(t, bt, mid_mid, "symbols.table.mid_mid"),
            mid_right: symbol!(t, bt, mid_right, "symbols.table.mid_right"),
            bottom_left: symbol!(t, bt, bottom_left, "symbols.table.bottom_left"),
            bottom_mid: symbol!(t, bt, bottom_mid, "symbols.table.bottom_mid"),
            bottom_right: symbol!(t, bt, bottom_right, "symbols.table.bottom_right"),
            vertical: symbol!(t, bt, vertical, "symbols.table.vertical"),
            horizontal: symbol!(t, bt, horizontal, "symbols.table.horizontal"),
        },
    })
}

/// A symbol in the file that cannot be drawn safely.
struct Invalid {
    path: &'static str,
    span: std::ops::Range<usize>,
    message: String,
}

/// What a symbol must satisfy besides containing no control characters.
#[derive(Clone, Copy)]
enum Rule {
    /// Exactly one column: table layout assumes it.
    OneColumn,
    /// At least one column.
    Visible,
    /// May be empty (no line).
    Any,
}

/// Checks the symbols written in the file. Control characters would reach
/// the terminal as escape sequences, so none are allowed anywhere.
fn validate(symbols: &SymbolsFile) -> Result<(), Invalid> {
    let t = &symbols.table;
    let checks = [
        (
            &symbols.task_unchecked,
            "symbols.task_unchecked",
            Rule::Visible,
        ),
        (&symbols.task_checked, "symbols.task_checked", Rule::Visible),
        (&symbols.quote_bar, "symbols.quote_bar", Rule::Visible),
        (&symbols.rule, "symbols.rule", Rule::Visible),
        (&symbols.h1_rule, "symbols.h1_rule", Rule::Any),
        (&symbols.h2_rule, "symbols.h2_rule", Rule::Any),
        (&t.top_left, "symbols.table.top_left", Rule::OneColumn),
        (&t.top_mid, "symbols.table.top_mid", Rule::OneColumn),
        (&t.top_right, "symbols.table.top_right", Rule::OneColumn),
        (&t.mid_left, "symbols.table.mid_left", Rule::OneColumn),
        (&t.mid_mid, "symbols.table.mid_mid", Rule::OneColumn),
        (&t.mid_right, "symbols.table.mid_right", Rule::OneColumn),
        (&t.bottom_left, "symbols.table.bottom_left", Rule::OneColumn),
        (&t.bottom_mid, "symbols.table.bottom_mid", Rule::OneColumn),
        (
            &t.bottom_right,
            "symbols.table.bottom_right",
            Rule::OneColumn,
        ),
        (&t.vertical, "symbols.table.vertical", Rule::OneColumn),
        (&t.horizontal, "symbols.table.horizontal", Rule::OneColumn),
    ];
    for (symbol, path, rule) in checks {
        if let Some(symbol) = symbol {
            check(symbol.get_ref(), rule).map_err(|message| Invalid {
                path,
                span: symbol.span(),
                message,
            })?;
        }
    }
    if let Some(bullets) = &symbols.bullets {
        let invalid = |message: String| Invalid {
            path: "symbols.bullets",
            span: bullets.span(),
            message,
        };
        if bullets.get_ref().is_empty() {
            return Err(invalid("needs at least one bullet".to_string()));
        }
        for bullet in bullets.get_ref() {
            check(bullet, Rule::Visible).map_err(invalid)?;
        }
    }
    Ok(())
}

fn check(symbol: &str, rule: Rule) -> Result<(), String> {
    if symbol.chars().any(char::is_control) {
        return Err(format!("{symbol:?} must not contain control characters"));
    }
    match rule {
        Rule::OneColumn if text_width(symbol) != 1 => {
            Err(format!("{symbol:?} must be exactly one column wide"))
        }
        Rule::Visible if text_width(symbol) == 0 => Err("must not be empty".to_string()),
        _ => Ok(()),
    }
}
