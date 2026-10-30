//! Streaming output: committed lines go to the terminal once and scroll
//! into its history; the active area below them is redrawn in place.
//!
//! No raw mode: Ctrl-C reaches the whole pipeline from the shell, so the
//! upstream program stops too, and tmd only has to restore the cursor.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveToColumn, MoveToPreviousLine};
use crossterm::queue;
use crossterm::terminal::{BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate};
use tmd_core::{Frame, Line, Streamer};

use crate::ansi::to_ansi;

/// At most one frame per this interval; input arriving meanwhile is held.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const READ_BUFFER: usize = 8 * 1024;

/// Whether the Unicode version was pushed and must be popped on exit.
static UNICODE_PUSHED: AtomicBool = AtomicBool::new(false);

/// Turns input text into frames: plain markdown (`Streamer`) or Claude
/// Code events (`ClaudeStream`).
pub trait Source {
    fn push(&mut self, chunk: &str) -> Frame;
    fn finish(&mut self) -> Frame;
    fn set_width(&mut self, width: u16);
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

/// Streams `input` to stdout until it ends. `make_source` builds the
/// source for the starting width; `fixed_width` disables following the
/// terminal's width.
pub fn run(
    input: Box<dyn Read + Send>,
    make_source: impl FnOnce(u16) -> Box<dyn Source>,
    fixed_width: Option<u16>,
) -> io::Result<()> {
    let chunks = spawn_reader(input);
    let mut out = io::stdout().lock();
    let _restore = Restore::start(&mut out)?;

    let (mut width, mut height) = terminal_size();
    if let Some(fixed) = fixed_width {
        width = fixed;
    }
    let mut streamer = make_source(width);
    let mut decoder = Utf8Decoder::default();
    let mut screen = ActiveArea::default();
    let mut pending = String::new();
    let mut last_frame = Instant::now() - FRAME_INTERVAL;

    loop {
        // With nothing to draw, sleep until input arrives; otherwise wait at
        // most until the next frame is due.
        let received = if pending.is_empty() {
            chunks.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            chunks.recv_timeout(FRAME_INTERVAL.saturating_sub(last_frame.elapsed()))
        };
        match received {
            Ok(Ok(bytes)) => pending.push_str(&decoder.feed(&bytes)),
            Ok(Err(err)) => return Err(err),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if !pending.is_empty() && last_frame.elapsed() >= FRAME_INTERVAL {
            if fixed_width.is_none() {
                (width, height) = terminal_size();
                streamer.set_width(width);
            }
            let frame = streamer.push(&std::mem::take(&mut pending));
            screen.draw(&mut out, &frame, height)?;
            last_frame = Instant::now();
        }
    }

    pending.push_str(&decoder.finish());
    if !pending.is_empty() {
        let frame = streamer.push(&pending);
        screen.draw(&mut out, &frame, height)?;
    }
    let last = streamer.finish();
    screen.draw(&mut out, &last, height)
}

fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size()
        .ok()
        .filter(|&(cols, rows)| cols > 0 && rows > 0)
        .unwrap_or((crate::cli::FALLBACK_WIDTH, 24))
}

/// Reads `input` on its own thread; the channel closes at end of input.
fn spawn_reader(mut input: Box<dyn Read + Send>) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = vec![0; READ_BUFFER];
        loop {
            match input.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if send.send(Ok(buffer[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => {
                    let _ = send.send(Err(err));
                    break;
                }
            }
        }
    });
    receive
}

/// The rows of the active area currently on screen, right above the
/// cursor.
#[derive(Default)]
pub struct ActiveArea {
    shown: usize,
}

impl ActiveArea {
    /// Replaces the active area with `frame`: back to its first row, clear
    /// to the end of the screen, print the newly committed lines (they
    /// scroll into the terminal's history), then the new active rows, at
    /// most `height - 1` of them (the last ones).
    pub fn draw(&mut self, out: &mut impl Write, frame: &Frame, height: u16) -> io::Result<()> {
        queue!(out, BeginSynchronizedUpdate)?;
        if self.shown > 0 {
            queue!(out, MoveToPreviousLine(self.shown as u16))?;
        } else {
            queue!(out, MoveToColumn(0))?;
        }
        queue!(out, Clear(ClearType::FromCursorDown))?;
        out.write_all(to_ansi(&frame.committed, true).as_bytes())?;
        // The cursor's own row is below the active rows, so a one-row
        // terminal has no room for any.
        let room = usize::from(height.saturating_sub(1));
        let visible: &[Line] = &frame.active[frame.active.len().saturating_sub(room)..];
        out.write_all(to_ansi(visible, true).as_bytes())?;
        self.shown = visible.len();
        queue!(out, EndSynchronizedUpdate)?;
        out.flush()
    }
}

/// Hides the cursor and sets the Unicode version while streaming; undoes
/// both when dropped, on Ctrl-C and on panic.
struct Restore;

impl Restore {
    fn start(out: &mut impl Write) -> io::Result<Self> {
        if matches!(
            std::env::var("TERM_PROGRAM").as_deref(),
            Ok("WezTerm") | Ok("Kaku")
        ) {
            // Emoji with variation selectors are two columns in Unicode 14,
            // matching how tmd measures them.
            out.write_all(b"\x1b]1337;UnicodeVersion=push\x07\x1b]1337;UnicodeVersion=14\x07")?;
            UNICODE_PUSHED.store(true, Ordering::SeqCst);
        }
        queue!(out, Hide)?;
        out.flush()?;
        // Ctrl-C ends the whole pipeline; put the terminal back first. The
        // main thread holds stdout, so write to the controlling terminal,
        // or to stderr when there is none.
        let _ = ctrlc::set_handler(|| {
            let sequence = restore_sequence();
            let wrote = std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/tty")
                .and_then(|mut tty| tty.write_all(sequence.as_bytes()));
            if wrote.is_err() {
                let _ = io::stderr().write_all(sequence.as_bytes());
            }
            std::process::exit(130);
        });
        Ok(Restore)
    }
}

impl Drop for Restore {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = out.write_all(restore_sequence().as_bytes());
        let _ = out.flush();
    }
}

/// Ends a synchronized update that may have been cut off, shows the
/// cursor and pops the Unicode version if it was pushed.
fn restore_sequence() -> String {
    let mut sequence = String::from("\x1b[?2026l\x1b[?25h");
    if UNICODE_PUSHED.swap(false, Ordering::SeqCst) {
        sequence.push_str("\x1b]1337;UnicodeVersion=pop\x07");
    }
    sequence
}

/// Decodes UTF-8 that arrives in arbitrary pieces: a character cut across
/// two reads waits for the rest; invalid bytes become U+FFFD.
#[derive(Default)]
pub struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    return out;
                }
                Err(err) => {
                    let valid = err.valid_up_to();
                    out.push_str(std::str::from_utf8(&self.pending[..valid]).unwrap_or_default());
                    match err.error_len() {
                        // An incomplete character at the end: keep it.
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                        Some(bad) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            self.pending.drain(..valid + bad);
                        }
                    }
                }
            }
        }
    }

    /// Whatever is left at the end of input, lossily.
    pub fn finish(&mut self) -> String {
        let rest = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        rest
    }
}

#[cfg(test)]
mod tests {
    use tmd_core::{Span, Style};

    use super::*;

    fn line(text: &str) -> Line {
        Line {
            spans: vec![Span::new(text, Style::default())],
            src: 0..0,
        }
    }

    fn frame(committed: &[&str], active: &[&str]) -> Frame {
        Frame {
            committed: committed.iter().map(|t| line(t)).collect(),
            active: active.iter().map(|t| line(t)).collect(),
        }
    }

    fn drawn(area: &mut ActiveArea, f: &Frame, height: u16) -> String {
        let mut out = Vec::new();
        area.draw(&mut out, f, height).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn frames_redraw_only_the_active_area() {
        let mut area = ActiveArea::default();
        let first = drawn(&mut area, &frame(&[], &["a1", "a2"]), 24);
        assert!(first.starts_with("\x1b[?2026h"), "synchronized update");
        assert!(!first.contains("\x1b[2F"), "nothing to move back over yet");
        assert!(first.contains("\x1b[J") && first.contains("a1\na2\n"));
        assert!(first.ends_with("\x1b[?2026l"));

        let second = drawn(&mut area, &frame(&["a1"], &["a2b"]), 24);
        assert!(
            second.contains("\x1b[2F\x1b[J"),
            "back over the two shown rows"
        );
        let committed = second.find("a1\n").unwrap();
        let active = second.find("a2b\n").unwrap();
        assert!(committed < active, "committed rows come first");

        let third = drawn(&mut area, &frame(&[], &[]), 24);
        assert!(third.contains("\x1b[1F\x1b[J"));
    }

    #[test]
    fn active_area_shows_only_the_rows_that_fit() {
        let mut area = ActiveArea::default();
        let out = drawn(&mut area, &frame(&[], &["r1", "r2", "r3", "r4", "r5"]), 4);
        assert!(!out.contains("r1") && !out.contains("r2"));
        assert!(out.contains("r3\nr4\nr5\n"), "the last height - 1 rows");
        let next = drawn(&mut area, &frame(&[], &[]), 4);
        assert!(
            next.contains("\x1b[3F"),
            "moves back over the three shown rows"
        );
    }

    /// A one-row terminal has no room for active rows: showing one
    /// pushed it into history on every frame.
    #[test]
    fn one_row_terminal_shows_no_active_rows() {
        let mut area = ActiveArea::default();
        let out = drawn(&mut area, &frame(&["c1"], &["a1"]), 1);
        assert!(out.contains("c1\n") && !out.contains("a1"));
        let next = drawn(&mut area, &frame(&[], &["a2"]), 1);
        assert!(
            !next.contains("F"),
            "nothing shown, nothing to move back over"
        );
    }

    #[test]
    fn utf8_split_across_reads_is_reassembled() {
        let mut decoder = Utf8Decoder::default();
        let bytes = "中文".as_bytes();
        assert_eq!(decoder.feed(&bytes[..2]), "");
        assert_eq!(decoder.feed(&bytes[2..4]), "中");
        assert_eq!(decoder.feed(&bytes[4..]), "文");
        assert_eq!(decoder.finish(), "");
    }

    #[test]
    fn invalid_utf8_becomes_replacement_characters() {
        let mut decoder = Utf8Decoder::default();
        assert_eq!(decoder.feed(b"a\xffb"), "a\u{FFFD}b");
        assert_eq!(decoder.feed(&"é".as_bytes()[..1]), "");
        assert_eq!(decoder.finish(), "\u{FFFD}");
    }
}
