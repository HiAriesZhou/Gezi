use std::ops::Range;
use std::sync::Arc;

use crate::image::{ImageCell, ImageSource};
use crate::style::Style;
use crate::width::text_width;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    /// Byte ranges in a Document's searchable text; empty on decorative spans
    /// and on the ordinary render/streaming APIs. Elision can represent several
    /// disjoint ranges. These are not source ranges.
    pub text_ranges: Vec<Range<usize>>,
    /// Index in Document::code_blocks(), including labels and empty code rows.
    /// None on ordinary render/streaming spans and non-code decorations.
    pub code_block: Option<usize>,
    /// Retained image metadata. Text output continues to use the alt label.
    pub image: Option<Arc<ImageSource>>,
    /// A reserved image row in an explicitly requested image layout.
    pub image_cell: Option<ImageCell>,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            text_ranges: Vec::new(),
            code_block: None,
            image: None,
            image_cell: None,
        }
    }

    pub(crate) fn slice(&self, range: Range<usize>) -> Self {
        let text_ranges = match self.text_ranges.as_slice() {
            [origin] if origin.len() == self.text.len() => {
                std::iter::once(origin.start + range.start..origin.start + range.end).collect()
            }
            ranges => ranges.to_vec(),
        };
        Self {
            text: self.text[range].to_owned(),
            style: self.style.clone(),
            text_ranges,
            code_block: self.code_block,
            image: self.image.clone(),
            image_cell: self.image_cell,
        }
    }

    pub fn width(&self) -> usize {
        text_width(&self.text)
    }
}

/// Size of a logical line, independent of the output protocol.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineSize {
    #[default]
    Normal,
    /// Twice the normal width and height.
    Double,
}

/// One rendered logical line (one or two physical rows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
    /// Source byte range of the top-level block this row belongs to. Rows
    /// wrapped from the same block share it; a separating blank row belongs
    /// to the following block. Top-level code rows use their own line ranges.
    pub src: Range<usize>,
    pub size: LineSize,
}

impl Line {
    /// Apply search styles to semantic matches without changing layout or clusters.
    /// `matches` must be ordered, non-overlapping ranges from `Document::find`.
    pub fn highlight_matches(
        &self,
        matches: &[Range<usize>],
        selected: Option<&Range<usize>>,
        normal: &Style,
        active: &Style,
    ) -> Self {
        let intersects = |range: &Range<usize>| {
            let index = matches.partition_point(|m| m.end <= range.start);
            matches.get(index).is_some_and(|m| m.start < range.end)
        };
        let mut marked = self.clone();
        marked.spans.clear();
        for span in &self.spans {
            if !span.text_ranges.iter().any(&intersects) {
                marked.spans.push(span.clone());
                continue;
            }
            let mut offset = 0;
            for cluster in crate::clusters(&span.text) {
                let end = offset + cluster.len();
                let mut piece = span.slice(offset..end);
                if piece.text_ranges.iter().any(&intersects) {
                    let current = selected.is_some_and(|m| {
                        piece
                            .text_ranges
                            .iter()
                            .any(|r| r.start < m.end && r.end > m.start)
                    });
                    piece.style = piece.style.patch(if current { active } else { normal });
                    piece.style.dim = false;
                }
                // Merge adjacent pieces of the same style to keep ANSI output compact.
                if let Some(previous) = marked.spans.last_mut()
                    && previous.style == piece.style
                    && previous.code_block == piece.code_block
                    && previous.image == piece.image
                    && previous.image_cell == piece.image_cell
                    && previous.text_ranges.len() == 1
                    && piece.text_ranges.len() == 1
                    && previous.text_ranges[0].end == piece.text_ranges[0].start
                {
                    previous.text.push_str(&piece.text);
                    previous.text_ranges[0].end = piece.text_ranges[0].end;
                } else {
                    marked.spans.push(piece);
                }
                offset = end;
            }
        }
        marked
    }

    /// The row's text without styling.
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(Span::width).sum::<usize>() * self.physical_height()
    }

    pub fn physical_height(&self) -> usize {
        match self.size {
            LineSize::Normal => 1,
            LineSize::Double => 2,
        }
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use crate::{Color, Document, RenderOptions, Theme};

    #[test]
    fn highlights_exact_matches_across_styles_wraps_and_unicode() {
        for (md, query) in [
            ("needle needle", "needle"),
            ("nee**dl**e", "needle"),
            ("中文中文", "中文"),
            ("é 👩‍💻 é 👩‍💻", "👩"),
            ("```rust\nlet needle = 1;\n```", "needle"),
            ("# needle", "needle"),
        ] {
            for width in [6, 80] {
                let doc = Document::new(md, Theme::default());
                let matches = doc.find(query);
                assert!(!matches.is_empty());
                let normal = Style::fg(Color::Ansi(0)).on(Color::Ansi(11));
                let active = normal.clone().bold().underline();
                let lines = doc.layout(
                    width,
                    RenderOptions {
                        large_headings: true,
                    },
                );
                let mut highlighted = String::new();
                for line in lines {
                    let marked =
                        line.highlight_matches(&matches, matches.first(), &normal, &active);
                    assert_eq!(marked.plain_text(), line.plain_text());
                    assert_eq!(marked.width(), line.width());
                    assert_eq!(marked.size, line.size);
                    for span in marked.spans {
                        if span.style.bg == Color::Ansi(11) {
                            highlighted.push_str(&span.text);
                        }
                    }
                }
                let expected = if query == "👩" {
                    "👩‍💻"
                } else {
                    query
                };
                assert_eq!(highlighted, expected.repeat(matches.len()));
            }
        }
    }

    #[test]
    fn clipped_table_match_highlights_ellipsis_only_and_keeps_metadata() {
        let doc = Document::new(
            "| column |\n|---|\n| prefix needle suffix |",
            Theme::default(),
        );
        let matches = doc.find("needle");
        let style = Style::fg(Color::Ansi(0)).on(Color::Ansi(11));
        let lines = doc.layout(10, RenderOptions::default());
        let row = Document::match_line(&lines, &matches[0]).unwrap();
        let marked = lines[row].highlight_matches(&matches, None, &style, &style);
        assert_eq!(marked.plain_text(), lines[row].plain_text());
        assert!(marked.spans.iter().any(|s| s.style.bg == style.bg));
        assert!(
            marked
                .spans
                .iter()
                .filter(|s| s.style.bg == style.bg)
                .all(|s| !s.text_ranges.is_empty())
        );
    }
}
