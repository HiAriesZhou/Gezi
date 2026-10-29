//! Markdown rendering core for tmd.
//!
//! Turns markdown into styled lines. This crate must not depend on any
//! terminal library; output backends live in `tmd-cli`.

use std::ops::Range;

mod code;
mod highlight;
mod html;
mod layout;
mod line;
mod parse;
mod style;
mod table;
mod theme;
mod width;
mod wrap;

pub use line::{Line, Span};
pub use style::{Color, Style};
pub use theme::{Symbols, TableSymbols, Theme};

/// Renders `md` into rows at most `width` columns wide.
///
/// Top-level blocks are separated by one blank row, which belongs to the
/// block above it.
pub fn render(md: &str, width: u16, theme: &Theme) -> Vec<Line> {
    let width = usize::from(width.max(1));
    let ctx = layout::Ctx {
        theme,
        list_depth: 0,
    };
    let top = parse::parse(md, theme);
    let mut lines = Vec::new();
    for (i, top_block) in top.iter().enumerate() {
        if i > 0 {
            lines.push(Line {
                spans: Vec::new(),
                src: separator_src(&top[i - 1], lines.last()),
            });
        }
        if let parse::Block::Code(code) = &top_block.block {
            // Code lines keep their own source ranges so streaming can
            // commit a long block line by line.
            lines.extend(
                code::rows(theme, code, width)
                    .into_iter()
                    .map(|(spans, src)| Line { spans, src }),
            );
            continue;
        }
        lines.extend(
            layout::block(&ctx, &top_block.block, width)
                .into_iter()
                .map(|spans| Line {
                    spans,
                    src: top_block.src.clone(),
                }),
        );
    }
    lines
}

/// Source range of the blank row after `block`: the block's own range,
/// except after a code block, whose rows are committed line by line while
/// streaming. There it covers only what follows the last code row (the
/// closing fence), so it still starts after the committed offset.
fn separator_src(block: &parse::TopBlock, last_row: Option<&Line>) -> Range<usize> {
    match (&block.block, last_row) {
        (parse::Block::Code(_), Some(row)) => row.src.end.min(block.src.end)..block.src.end,
        _ => block.src.clone(),
    }
}

#[cfg(test)]
mod tests;
