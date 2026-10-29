use std::ops::Range;

use crate::style::Style;
use crate::width::text_width;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    pub fn width(&self) -> usize {
        text_width(&self.text)
    }
}

/// One rendered terminal row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
    /// Source byte range of the top-level block this row belongs to. Rows
    /// wrapped from the same block share it; the blank row after a block
    /// belongs to that block.
    pub src: Range<usize>,
}

impl Line {
    /// The row's text without styling.
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(Span::width).sum()
    }
}
