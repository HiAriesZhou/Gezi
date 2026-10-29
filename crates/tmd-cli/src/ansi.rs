//! Styled lines to text with ANSI SGR codes.

use std::fmt::Write;

use tmd_core::{Color, Line, Style};

const RESET: &str = "\x1b[0m";

/// Converts lines to text, one row per `\n`-terminated line. With
/// `hyperlinks`, linked text is wrapped in OSC 8 so terminals make it
/// clickable.
pub fn to_ansi(lines: &[Line], hyperlinks: bool) -> String {
    let mut out = String::new();
    for line in lines {
        write_line(&mut out, line, hyperlinks);
        out.push('\n');
    }
    out
}

fn write_line(out: &mut String, line: &Line, hyperlinks: bool) {
    let mut open_link: Option<&str> = None;
    for span in &line.spans {
        let link = span.style.link.as_deref().filter(|_| hyperlinks);
        if link != open_link {
            if open_link.is_some() {
                out.push_str("\x1b]8;;\x1b\\");
            }
            if let Some(url) = link {
                let _ = write!(out, "\x1b]8;;{url}\x1b\\");
            }
            open_link = link;
        }
        let sgr = sgr(&span.style);
        if sgr.is_empty() {
            out.push_str(&span.text);
        } else {
            let _ = write!(out, "\x1b[{sgr}m{}{RESET}", span.text);
        }
    }
    if open_link.is_some() {
        out.push_str("\x1b]8;;\x1b\\");
    }
}

/// SGR parameters for `style`, separated by `;`; empty for the default style.
fn sgr(style: &Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    let flags = [
        (style.bold, "1"),
        (style.dim, "2"),
        (style.italic, "3"),
        (style.underline, "4"),
        (style.strike, "9"),
    ];
    codes.extend(
        flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, code)| code.to_string()),
    );
    codes.extend(color(style.fg, 30, 90, 38));
    codes.extend(color(style.bg, 40, 100, 48));
    codes.join(";")
}

/// SGR code for a color: `base` for ANSI 0–7, `bright` for 8–15 and
/// `extended` for 24-bit.
fn color(color: Color, base: u8, bright: u8, extended: u8) -> Option<String> {
    match color {
        Color::Reset => None,
        Color::Ansi(n @ 0..=7) => Some((base + n).to_string()),
        Color::Ansi(n) => Some((bright + (n.min(15) - 8)).to_string()),
        Color::Rgb(r, g, b) => Some(format!("{extended};2;{r};{g};{b}")),
    }
}

#[cfg(test)]
mod tests {
    use tmd_core::Span;

    use super::*;

    fn line(spans: Vec<Span>) -> Line {
        Line { spans, src: 0..0 }
    }

    #[test]
    fn plain_text_has_no_codes() {
        let out = to_ansi(&[line(vec![Span::new("hi", Style::default())])], true);
        assert_eq!(out, "hi\n");
    }

    #[test]
    fn styles_map_to_sgr_codes() {
        let style = Style::fg(Color::Ansi(9))
            .bold()
            .underline()
            .on(Color::Rgb(1, 2, 3));
        let out = to_ansi(&[line(vec![Span::new("x", style)])], false);
        assert_eq!(out, "\x1b[1;4;91;48;2;1;2;3mx\x1b[0m\n");
    }

    #[test]
    fn links_use_osc8_only_when_enabled() {
        let style = Style {
            link: Some("https://a.io".into()),
            ..Style::default()
        };
        let lines = [line(vec![
            Span::new("a", style.clone()),
            Span::new("b", style.bold()),
            Span::new(" c", Style::default()),
        ])];
        assert_eq!(
            to_ansi(&lines, true),
            "\x1b]8;;https://a.io\x1b\\a\x1b[1mb\x1b[0m\x1b]8;;\x1b\\ c\n"
        );
        assert_eq!(to_ansi(&lines, false), "a\x1b[1mb\x1b[0m c\n");
    }
}
