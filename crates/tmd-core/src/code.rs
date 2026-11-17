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

pub(crate) fn media_rows(
    theme: &Theme,
    code: &CodeBlock,
    width: usize,
    copy_buttons: bool,
    images: Option<&crate::ImageLayout<'_>>,
) -> Vec<(Row, Range<usize>)> {
    let mut source = rows(theme, code, width, copy_buttons);
    let (Some(image), Some(images)) = (&code.preview, images) else {
        return source;
    };
    let hidden = images.source_previews.contains(&image.index);
    let ready = images.sizes.get(image.index).is_some_and(Option::is_some);
    let label = if images.failed.get(image.index) == Some(&true) {
        "mermaid · preview unavailable · source"
    } else if ready && !hidden {
        "mermaid · native preview · m: source"
    } else {
        "mermaid · source · m: preview"
    };
    if let Some((row, _)) = source.first_mut() {
        *row = label_row(
            &theme.code_block,
            label,
            width.max(1),
            CODE_INSET.min(width.saturating_sub(2) / 2),
            code.index.filter(|_| copy_buttons && width >= 4),
        );
    }
    for (row, _) in &mut source {
        if let Some(span) = row.first_mut() {
            span.image = Some(image.clone());
            span.code_block = code.index;
        }
    }
    if hidden {
        return source;
    }
    if ready {
        let mut span = Span::new("Mermaid", theme.text.clone());
        span.image = Some(image.clone());
        span.text_ranges = code
            .lines
            .iter()
            .filter_map(|line| line.text_range.clone())
            .collect();
        let mut preview = crate::image::wrap(&[span], width, Some(images));
        for row in &mut preview {
            for span in row {
                span.code_block = code.index;
            }
        }
        source.truncate(1);
        source.extend(preview.into_iter().map(|row| (row, code.src.clone())));
    }
    source
}

/// Rows with the source range each comes from: the label row maps to the
/// opening fence, code rows to their source line.
pub(crate) fn rows(
    theme: &Theme,
    code: &CodeBlock,
    width: usize,
    copy_buttons: bool,
) -> Vec<(Row, Range<usize>)> {
    let style = &theme.code_block;
    let width = width.max(1);
    let inset = CODE_INSET.min(width.saturating_sub(2) / 2);
    let content_width = width - 2 * inset;
    let mut rows = Vec::new();
    let button = code.index.filter(|_| copy_buttons && width >= 4);
    if code.lang.is_some() || button.is_some() {
        let fence_end = code
            .lines
            .first()
            .map_or(code.src.end, |line| line.src.start);
        rows.push((
            label_row(
                style,
                code.lang.as_deref().unwrap_or(""),
                width,
                inset,
                button,
            ),
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

fn label_row(style: &Style, lang: &str, width: usize, inset: usize, button: Option<usize>) -> Row {
    let label_style = Style {
        dim: true,
        ..style.clone()
    };
    let Some(index) = button else {
        return padded(
            style,
            vec![Span::new(truncate(lang, width - 2 * inset), label_style)],
            width,
            inset,
        );
    };
    let button_text = if width >= 6 { " Copy " } else { "Copy" };
    let right = inset.min(width - button_text.len());
    let available = width - right - button_text.len();
    let left = inset.min(available);
    let label = truncate(lang, available.saturating_sub(left + 1));
    let mut row = vec![
        Span::new(" ".repeat(left), style.clone()),
        Span::new(label, label_style),
    ];
    let used: usize = row.iter().map(Span::width).sum();
    row.push(Span::new(" ".repeat(available - used), style.clone()));
    let mut control = Span::new(button_text, style.clone().bold().underline());
    control.code_copy = Some(index);
    row.push(control);
    row.push(Span::new(" ".repeat(right), style.clone()));
    row
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

#[cfg(test)]
mod tests {
    use crate::{Document, RenderOptions, Theme, render};

    #[test]
    fn copy_controls_are_opt_in_and_not_searchable() {
        let source = "```rust\nlet x = 1;\n```\n\n```\n```\n\n> ```\n> nested\n> ```\n\n- item\n\n  ```\n  listed\n  ```";
        let doc = Document::new(source, Theme::default());
        assert_eq!(doc.code_blocks().len(), 4);
        assert!(doc.find("Copy").is_empty());
        for lines in [
            render(source, 60, &Theme::default()),
            doc.layout(60, RenderOptions::default()),
        ] {
            assert!(
                lines
                    .iter()
                    .flat_map(|l| &l.spans)
                    .all(|s| s.code_copy.is_none())
            );
        }
        let lines = doc.layout(
            60,
            RenderOptions {
                code_copy_buttons: true,
                ..Default::default()
            },
        );
        let controls: Vec<_> = lines
            .iter()
            .flat_map(|l| &l.spans)
            .filter(|s| s.code_copy.is_some())
            .collect();
        assert_eq!(
            controls
                .iter()
                .map(|s| s.code_copy.unwrap())
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert!(
            controls
                .iter()
                .all(|s| s.text_ranges.is_empty() && s.text.trim() == "Copy")
        );
    }

    #[test]
    fn narrow_code_headers_fit_without_overwriting_content() {
        let doc = Document::new("```long_language_name\nabc\n```", Theme::default());
        for width in 1..40 {
            let lines = doc.layout(
                width,
                RenderOptions {
                    code_copy_buttons: true,
                    ..Default::default()
                },
            );
            assert!(lines.iter().all(|l| l.width() <= usize::from(width)));
            let controls: Vec<_> = lines
                .iter()
                .flat_map(|l| &l.spans)
                .filter(|s| s.code_copy.is_some())
                .collect();
            assert_eq!(controls.len(), usize::from(width >= 4));
            assert_eq!(doc.code_blocks()[0].text, "abc\n");
        }
    }
}
