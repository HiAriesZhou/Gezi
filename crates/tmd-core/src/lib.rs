//! Markdown rendering core for tmd.
//!
//! Turns markdown into styled lines. This crate must not depend on any
//! terminal library; output backends live in `tmd-cli`.

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
                src: top[i - 1].src.clone(),
            });
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

#[cfg(test)]
mod tests;
