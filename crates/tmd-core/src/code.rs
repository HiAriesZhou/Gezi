//! Code block layout: a label row with the language, then each source
//! line on a full-width background, wrapped when longer than a row.

use std::ops::Range;

use crate::highlight::highlight;
use crate::line::Span;
use crate::parse::CodeBlock;
use crate::style::Style;
use crate::theme::Theme;
use crate::width::truncate;
use crate::wrap::{Row, hard_wrap_spans, push_text};

const CODE_INSET: usize = 2;

/// Rows with the source range each comes from: the label row maps to the
/// opening fence, code rows to their source line.
pub(crate) fn rows(theme: &Theme, code: &CodeBlock, width: usize) -> Vec<(Row, Range<usize>)> {
    let style = &theme.code_block;
    let width = width.max(1);
    let inset = CODE_INSET.min(width.saturating_sub(2) / 2);
    let content_width = width - 2 * inset;
    let mut rows = Vec::new();
    if let Some(lang) = &code.lang {
        let fence_end = code
            .lines
            .first()
            .map_or(code.src.end, |line| line.src.start);
        rows.push((
            label_row(style, lang, width, inset),
            code.src.start..fence_end,
        ));
    }
    for (line, spans) in code.lines.iter().zip(styled_lines(theme, code)) {
        for row in hard_wrap_spans(&spans, content_width) {
            rows.push((padded(style, row, width, inset), line.src.clone()));
        }
    }
    if code.lines.is_empty() {
        rows.push((padded(style, Row::new(), width, inset), code.src.clone()));
    }
    for (row, _) in &mut rows {
        if let Some(span) = row.first_mut() {
            span.code_block = code.index;
        }
    }
    rows
}

fn label_row(style: &Style, lang: &str, width: usize, inset: usize) -> Row {
    let label = truncate(lang, width - 2 * inset);
    let label_style = Style {
        dim: true,
        ..style.clone()
    };
    padded(style, vec![Span::new(label, label_style)], width, inset)
}

/// Each line as spans on the block background: highlighted when the
/// language is known, plain otherwise.
fn styled_lines(theme: &Theme, code: &CodeBlock) -> Vec<Vec<Span>> {
    let style = &theme.code_block;
    let texts: Vec<&str> = code.lines.iter().map(|line| line.text.as_str()).collect();
    let highlighted = code
        .lang
        .as_deref()
        .and_then(|lang| highlight(lang, &texts, theme.dark));
    let mut styled: Vec<Vec<Span>> = match highlighted {
        Some(lines) => lines
            .into_iter()
            .map(|pieces| {
                pieces
                    .into_iter()
                    .map(|(text, color)| Span::new(text, style.patch(&color)))
                    .collect()
            })
            .collect(),
        None => texts
            .iter()
            .map(|text| vec![Span::new(*text, style.clone())])
            .collect(),
    };
    for (line, spans) in code.lines.iter().zip(&mut styled) {
        if let Some(range) = &line.text_range {
            let mut offset = range.start;
            for span in spans {
                span.text_ranges = std::iter::once(offset..offset + span.text.len()).collect();
                offset += span.text.len();
            }
        }
    }
    styled
}

/// `row` filled to `width` with the block background.
fn padded(style: &Style, row: Row, width: usize, inset: usize) -> Row {
    let mut padded = Row::new();
    push_text(&mut padded, &" ".repeat(inset), style);
    padded.extend(row);
    let used: usize = padded.iter().map(Span::width).sum();
    push_text(&mut padded, &" ".repeat(width.saturating_sub(used)), style);
    padded
}
