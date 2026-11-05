//! Input adapters shared by one-shot and streaming output.
use tmd_core::{Frame, Streamer};

/// Turns input text into frames: plain markdown (`Streamer`) or Claude
/// Code events (`ClaudeStream`).
pub trait Source {
    fn push(&mut self, chunk: &str) -> Frame;
    fn finish(&mut self) -> Frame;
    fn set_width(&mut self, width: u16);
    fn take_diagnostics(&mut self) -> Vec<String> {
        Vec::new()
    }
}

impl Source for Streamer {
    fn push(&mut self, chunk: &str) -> Frame {
        Streamer::push(self, chunk)
    }

    fn finish(&mut self) -> Frame {
        Streamer::finish(self)
    }

    fn set_width(&mut self, width: u16) {
        Streamer::set_width(self, width);
    }
}
