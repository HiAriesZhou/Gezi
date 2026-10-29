//! Streaming: markdown arrives in chunks; lines that can no longer change
//! are committed once, the rest form an active area that is redrawn.

use pulldown_cmark::{Event, Tag};

use crate::line::Line;
use crate::parse::events;
use crate::render;
use crate::theme::Theme;

/// What one push or finish produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Frame {
    /// Lines committed by this call; they never change or repeat.
    pub committed: Vec<Line>,
    /// Every line of the active area, which the next call replaces.
    pub active: Vec<Line>,
}

/// Renders a growing markdown buffer. Each call re-parses the whole buffer
/// (there is no incremental parser) and splits the lines at a byte
/// boundary before which the source can no longer change.
#[derive(Debug, Clone)]
pub struct Streamer {
    buffer: String,
    /// Source offset everything before which has been committed.
    committed: usize,
    width: u16,
    theme: Theme,
}

impl Streamer {
    pub fn new(width: u16, theme: Theme) -> Self {
        Self {
            buffer: String::new(),
            committed: 0,
            width,
            theme,
        }
    }

    /// Width for the next frames; committed lines keep the old width.
    pub fn set_width(&mut self, width: u16) {
        self.width = width;
    }

    pub fn push(&mut self, chunk: &str) -> Frame {
        self.buffer.push_str(chunk);
        let boundary = boundary(&self.buffer);
        self.frame(boundary)
    }

    /// Commits everything left; the active area ends up empty.
    pub fn finish(&mut self) -> Frame {
        self.frame(usize::MAX)
    }

    fn frame(&mut self, boundary: usize) -> Frame {
        let boundary = boundary.max(self.committed);
        let mut frame = Frame::default();
        for line in render(&self.buffer, self.width, &self.theme) {
            if line.src.end > boundary {
                frame.active.push(line);
            } else if line.src.start >= self.committed {
                frame.committed.push(line);
            }
            // Otherwise the line was committed by an earlier call.
        }
        self.committed = boundary;
        frame
    }
}

/// The offset up to which `md` can no longer change: the start of the
/// last top-level block, moved back to the start of its line. When that
/// block is a code block, its complete lines are final too, so the
/// boundary is the end of its last complete line; with no such line yet
/// it stays at the block's start, keeping the label and empty-block rows
/// active.
///
/// Only complete lines are parsed: an unfinished last line can look like
/// a new block (`| 中` after a table) and turn back into part of the
/// previous block once it ends, so it must not decide what is final.
pub(crate) fn boundary(md: &str) -> usize {
    let md = &md[..md.rfind('\n').map_or(0, |i| i + 1)];
    let mut depth = 0usize;
    let mut last_start = None;
    let mut last_is_code = false;
    let mut last_code_line_end = None;
    for (event, range) in events(md) {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    last_start = Some(range.start);
                    last_is_code = matches!(tag, Tag::CodeBlock(_));
                    last_code_line_end = None;
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Rule if depth == 0 => {
                last_start = Some(range.start);
                last_is_code = false;
            }
            Event::Text(text) if depth == 1 && last_is_code && text.ends_with('\n') => {
                // Only complete lines are parsed, so code text ends with a
                // newline at the end of its last line.
                last_code_line_end = Some(range.end);
            }
            _ => {}
        }
    }
    match (last_start, last_code_line_end) {
        (None, _) => 0,
        (Some(_), Some(end)) if last_is_code => end,
        (Some(start), _) => md[..start].rfind('\n').map_or(0, |i| i + 1),
    }
}
