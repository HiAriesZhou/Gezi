//! Static document text and its one-way mapping into laid-out spans.
use crate::{
    Line, RenderOptions, Span, Theme,
    parse::{self, Block, TopBlock},
    render_blocks,
};
use std::ops::Range;

pub struct Document {
    blocks: Vec<TopBlock>,
    text: String,
    theme: Theme,
}

impl Document {
    pub fn new(markdown: &str, theme: Theme) -> Self {
        let mut blocks = parse::parse(markdown, &theme);
        let mut text = String::new();
        for block in &mut blocks {
            index_block(&mut block.block, &mut text);
        }
        Self {
            blocks,
            text,
            theme,
        }
    }

    pub fn layout(&self, width: u16, options: RenderOptions) -> Vec<Line> {
        render_blocks(&self.blocks, width, &self.theme, options)
    }

    /// Literal, case-sensitive matches in semantic text, independent of wrapping.
    pub fn find(&self, query: &str) -> Vec<Range<usize>> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.text
            .match_indices(query)
            .map(|(i, value)| i..i + value.len())
            .collect()
    }

    /// First logical content line representing a match. Clipped table content
    /// maps to that row's ellipsis rather than to a different source block.
    pub fn match_line(lines: &[Line], range: &Range<usize>) -> Option<usize> {
        lines.iter().position(|line| {
            line.spans.iter().any(|span| {
                span.text_ranges
                    .iter()
                    .any(|r| r.start < range.end && r.end > range.start)
            })
        })
    }
}

fn index_spans(spans: &mut [Span], text: &mut String) {
    text.push('\n');
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        span.text_ranges = std::iter::once(start..text.len()).collect();
    }
}

fn index_block(block: &mut Block, text: &mut String) {
    match block {
        Block::Paragraph(spans) | Block::Heading { spans, .. } => index_spans(spans, text),
        Block::Code(code) => {
            for line in &mut code.lines {
                text.push('\n');
                let start = text.len();
                text.push_str(&line.text);
                line.text_range = Some(start..text.len());
            }
        }
        Block::List { items, .. } => {
            for item in items {
                for block in &mut item.blocks {
                    index_block(block, text);
                }
            }
        }
        Block::Quote(blocks) => {
            for block in blocks {
                index_block(block, text);
            }
        }
        Block::Table(table) => {
            for cell in &mut table.header {
                index_spans(cell, text);
            }
            for row in &mut table.rows {
                for cell in row {
                    index_spans(cell, text);
                }
            }
        }
        Block::Rule => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn searches_semantics_across_styles_and_wraps_without_decorations() {
        let doc = Document::new(
            "# Title\n\n中**文** English abcdefghij [label](hidden-url)\n",
            Theme::default(),
        );
        assert_eq!(doc.find("中文").len(), 1);
        assert!(doc.find("hidden-url").is_empty());
        assert!(doc.find("**").is_empty());
        let found = doc.find("abcdefghij");
        for width in [4, 8, 80] {
            let lines = doc.layout(width, RenderOptions::default());
            assert!(Document::match_line(&lines, &found[0]).is_some());
        }
    }
    #[test]
    fn nested_matches_map_to_content_not_the_container_start() {
        let doc = Document::new(
            "- one\n\n  many words before the matching needle\n\n> quoted needle\n",
            Theme::default(),
        );
        let lines = doc.layout(15, RenderOptions::default());
        let matches = doc.find("needle");
        assert_eq!(matches.len(), 2);
        for m in matches {
            let row = Document::match_line(&lines, &m).unwrap();
            assert!(lines[row].plain_text().contains("needle"));
        }
    }
    #[test]
    fn indexed_layout_preserves_existing_rendering_and_source_contract() {
        let md = include_str!("../../../fixtures/acceptance.md");
        let theme = Theme::default();
        let doc = Document::new(md, theme.clone());
        for width in [1, 4, 20, 80] {
            for large in [false, true] {
                let options = RenderOptions {
                    large_headings: large,
                };
                let normal = crate::render_with_options(md, width, &theme, options);
                let indexed = doc.layout(width, options);
                assert_eq!(normal.len(), indexed.len());
                for (a, b) in normal.iter().zip(indexed) {
                    assert_eq!(
                        (a.plain_text(), a.src.clone(), a.size),
                        (b.plain_text(), b.src, b.size)
                    );
                    assert!(a.spans.iter().all(|span| span.text_ranges.is_empty()));
                }
            }
        }
    }

    #[test]
    fn repeated_emoji_and_styled_table_cells_have_stable_matches() {
        let doc = Document::new(
            "# 👩‍💻 heading\n\n| **中文** needle | one |\n|---|---|\n| needle | 👩‍💻 |\n",
            Theme::default(),
        );
        assert_eq!(doc.find("needle").len(), 2);
        assert_eq!(doc.find("👩‍💻").len(), 2);
        for width in [1, 4, 20, 80] {
            let lines = doc.layout(
                width,
                RenderOptions {
                    large_headings: true,
                },
            );
            for query in ["needle", "👩‍💻", "中文"] {
                for found in doc.find(query) {
                    assert!(
                        Document::match_line(&lines, &found).is_some(),
                        "{query} at {width}"
                    );
                }
            }
        }
    }

    #[test]
    fn elided_table_ranges_never_include_other_wrapped_rows() {
        let doc = Document::new(
            "| abcdefghij | target |\n|---|---|\n| one | two |",
            Theme::default(),
        );
        let lines = doc.layout(5, RenderOptions::default());
        let found = doc.find("ef");
        let row = Document::match_line(&lines, &found[0]).unwrap();
        assert_eq!(lines[row].plain_text(), "│ ef…");
    }

    #[test]
    fn code_and_clipped_table_matches_keep_their_rows() {
        let doc = Document::new(
            "```rust\nfn main() { needle(); }\n```\n\n| a | hidden needle |\n|---|---|\n| b | x |",
            Theme::default(),
        );
        let lines = doc.layout(5, RenderOptions::default());
        for m in doc.find("needle") {
            assert!(Document::match_line(&lines, &m).is_some());
        }
    }
}
