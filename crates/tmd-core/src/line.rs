use std::ops::Range;

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
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
            text_ranges: Vec::new(),
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
