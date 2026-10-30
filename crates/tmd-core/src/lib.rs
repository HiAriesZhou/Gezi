//! Markdown rendering core for tmd.
//!
//! Turns markdown into styled lines. This crate must not depend on any
//! terminal library; output backends live in `tmd-cli`.

mod code;
mod highlight;
mod html;
mod layout;
mod line;
mod parse;
mod streamer;
mod style;
mod table;
mod theme;
mod theme_file;
mod width;
mod wrap;

pub use line::{Line, Span};
pub use streamer::{Frame, Streamer};
pub use style::{Color, Style};
pub use theme::{Symbols, TableSymbols, Theme};
pub use theme_file::ThemeError;
pub use width::{text_width, truncate};

/// Renders `md` into rows at most `width` columns wide.
///
/// Top-level blocks are separated by one blank row, which belongs to the
/// block below it: it shares the source range of that block's first row.
/// Whether the row exists depends on the block below rendering anything
/// (an HTML block of bare tags renders nothing), so streaming commits it
/// together with that block.
pub fn render(md: &str, width: u16, theme: &Theme) -> Vec<Line> {
    let width = usize::from(width.max(1));
    let ctx = layout::Ctx {
        theme,
        list_depth: 0,
    };
    let mut lines = Vec::new();
    for top_block in parse::parse(md, theme) {
        let rows: Vec<Line> = if let parse::Block::Code(code) = &top_block.block {
            // Code lines keep their own source ranges so streaming can
            // commit a long block line by line.
            code::rows(theme, code, width)
                .into_iter()
                .map(|(spans, src)| Line { spans, src })
                .collect()
        } else {
            layout::block(&ctx, &top_block.block, width)
                .into_iter()
                .map(|spans| Line {
                    spans,
                    src: top_block.src.clone(),
                })
                .collect()
        };
        if let (false, Some(first)) = (lines.is_empty(), rows.first()) {
            lines.push(Line {
                spans: Vec::new(),
                src: first.src.clone(),
            });
        }
        lines.extend(rows);
    }
    lines
}

#[cfg(test)]
mod tests;
