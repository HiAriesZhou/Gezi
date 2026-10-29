//! Code block layout: a label row with the language, then each source
//! line on a full-width background, wrapped when longer than a row.

use std::ops::Range;

use crate::line::Span;
use crate::parse::CodeBlock;
use crate::style::Style;
use crate::theme::Theme;
use crate::width::{text_width, truncate};
use crate::wrap::{Row, hard_wrap_lines, push_text};

/// Rows with the source range each comes from: the label row maps to the
/// opening fence, code rows to their source line.
pub(crate) fn rows(theme: &Theme, code: &CodeBlock, width: usize) -> Vec<(Row, Range<usize>)> {
    let style = &theme.code_block;
    let mut rows = Vec::new();
    if let Some(lang) = &code.lang {
        let fence_end = code
            .lines
            .first()
            .map_or(code.src.end, |line| line.src.start);
        rows.push((label_row(style, lang, width), code.src.start..fence_end));
    }
    for line in &code.lines {
        for text in hard_wrap_lines(&line.text, width) {
            rows.push((padded(style, text, width), line.src.clone()));
        }
    }
    if code.lines.is_empty() {
        rows.push((padded(style, String::new(), width), code.src.clone()));
    }
    rows
}

fn label_row(style: &Style, lang: &str, width: usize) -> Row {
    let label = truncate(lang, width);
    let label_style = Style {
        dim: true,
        ..style.clone()
    };
    let mut row = Row::new();
    push_text(&mut row, &" ".repeat(width - text_width(label)), style);
    push_text(&mut row, label, &label_style);
    row
}

fn padded(style: &Style, text: String, width: usize) -> Row {
    let pad = width.saturating_sub(text_width(&text));
    vec![Span::new(text + &" ".repeat(pad), style.clone())]
}
