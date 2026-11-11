//! Streaming output: committed lines go to the terminal once and scroll
//! into its history; the active area below them is redrawn in place.
//!
//! No raw mode: Ctrl-C reaches the whole pipeline from the shell, so the
//! upstream program stops too, and tmd only has to restore the cursor.

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveToColumn, MoveToPreviousLine};
use crossterm::queue;
use crossterm::terminal::{BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate};
use tmd_core::{Frame, Line};

use crate::ansi::to_ansi;
use crate::source::Source;

/// At most one frame per this interval; input arriving meanwhile is held.
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const READ_BUFFER: usize = 8 * 1024;

/// How streaming ended once the terminal has been restored.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Finished,
    /// A handled signal arrived; the caller should end the process by it.
    Signalled(i32),
}

/// Everything the output loop waits for. `Wake` only interrupts a wait:
/// the signal itself is read from a shared flag, so it is never queued
/// behind input that was read earlier.
enum Event {
    Input(Vec<u8>),
    Failed(io::Error),
    End,
    Wake,
}

/// Streams `input` to stdout until it ends. `make_source` builds the
/// source for the starting width; `fixed_width` disables following the
/// terminal's width.
pub fn run(
    input: Box<dyn Read + Send>,
    make_source: impl FnOnce(u16) -> Box<dyn Source>,
    fixed_width: Option<u16>,
) -> io::Result<Outcome> {
    let (events, received) = mpsc::channel();
    let mut out = io::stdout().lock();
    let restore = Restore::start(&mut out, events.clone())?;
    spawn_reader(input, events);
    let outcome = stream(
        &received,
        &restore.signal,
        &mut out,
        make_source,
        fixed_width,
    )?;
    drop(restore);
    Ok(outcome)
}

fn stream(
    received: &Receiver<Event>,
    signal: &AtomicI32,
    out: &mut impl Write,
    make_source: impl FnOnce(u16) -> Box<dyn Source>,
    fixed_width: Option<u16>,
) -> io::Result<Outcome> {
    let (mut width, mut height) = terminal_size();
    if let Some(fixed) = fixed_width {
        width = fixed;
    }
    let mut streamer = make_source(width);
    let mut decoder = Utf8Decoder::default();
    let mut screen = ActiveArea::default();
    let mut pending = String::new();
    let mut last_frame = Instant::now() - FRAME_INTERVAL;

    let signalled = || match signal.load(Ordering::SeqCst) {
        0 => None,
        signal => Some(Outcome::Signalled(signal)),
    };
    loop {
        if let Some(outcome) = signalled() {
            return Ok(outcome);
        }
        // Idle waits block: a signal also sends `Wake`, so cancellation
        // needs neither polling nor the producer closing stdin. The reader
        // thread may stay blocked after a signal.
        let event = if pending.is_empty() {
            received.recv().map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            received.recv_timeout(FRAME_INTERVAL.saturating_sub(last_frame.elapsed()))
        };
        match event {
            Ok(Event::Input(bytes)) => pending.push_str(&decoder.feed(&bytes)),
            Ok(Event::Failed(err)) => return Err(err),
            Ok(Event::End) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Event::Wake) | Err(RecvTimeoutError::Timeout) => {}
        }
        if !pending.is_empty() && last_frame.elapsed() >= FRAME_INTERVAL {
            if fixed_width.is_none() {
                (width, height) = terminal_size();
                streamer.set_width(width);
            }
            let frame = streamer.push(&std::mem::take(&mut pending));
            draw_source_frame(&mut screen, out, &mut *streamer, &frame, height)?;
            last_frame = Instant::now();
        }
    }

    if let Some(outcome) = signalled() {
        return Ok(outcome);
    }
    pending.push_str(&decoder.finish());
    if !pending.is_empty() {
        let frame = streamer.push(&pending);
        draw_source_frame(&mut screen, out, &mut *streamer, &frame, height)?;
    }
    let last = streamer.finish();
    draw_source_frame(&mut screen, out, &mut *streamer, &last, height)?;
    // A signal during the final frames still decides how the process ends.
    Ok(signalled().unwrap_or(Outcome::Finished))
}

/// stderr may share stdout's terminal. Clear the active rows before a
/// diagnostic moves its cursor, then redraw below the diagnostic.
fn draw_source_frame(
    screen: &mut ActiveArea,
    out: &mut impl Write,
    source: &mut dyn Source,
    frame: &Frame,
    height: u16,
) -> io::Result<()> {
    let diagnostics = source.take_diagnostics();
    if !diagnostics.is_empty() {
        screen.draw(out, &Frame::default(), height)?;
        let mut errors = io::stderr().lock();
        for diagnostic in diagnostics {
            writeln!(errors, "{diagnostic}")?;
        }
        errors.flush()?;
    }
    screen.draw(out, frame, height)
}

fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size()
        .ok()
        .filter(|&(cols, rows)| cols > 0 && rows > 0)
        .unwrap_or((crate::cli::FALLBACK_WIDTH, 24))
}

/// Reads `input` on its own thread and reports its end explicitly: the
/// signal thread keeps the channel open.
fn spawn_reader(mut input: Box<dyn Read + Send>, events: Sender<Event>) {
    thread::spawn(move || {
        let mut buffer = vec![0; READ_BUFFER];
        loop {
            let event = match input.read(&mut buffer) {
                Ok(0) => Event::End,
                Ok(n) => Event::Input(buffer[..n].to_vec()),
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => Event::Failed(err),
            };
            let last = !matches!(event, Event::Input(_));
            if events.send(event).is_err() || last {
                break;
            }
        }
    });
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

/// The output thread owns restoration; signal handlers only queue an event.
/// This keeps cleanup from interleaving with a frame or waiting on its lock.
struct Restore {
    /// The first handled signal, or 0.
    signal: Arc<AtomicI32>,
    unicode_pushed: bool,
    #[cfg(unix)]
    signals: signal_hook::iterator::Handle,
}

impl Restore {
    fn start(out: &mut impl Write, events: Sender<Event>) -> io::Result<Self> {
        // Register before changing terminal state.
        let mut restore = Self::signals(events)?;
        if matches!(
            std::env::var("TERM_PROGRAM").as_deref(),
            Ok("WezTerm") | Ok("Kaku")
        ) {
            // Emoji with variation selectors are two columns in Unicode 14,
            // matching how tmd measures them.
            out.write_all(b"\x1b]1337;UnicodeVersion=push\x07\x1b]1337;UnicodeVersion=14\x07")?;
            restore.unicode_pushed = true;
        }
        queue!(out, Hide)?;
        out.flush()?;
        Ok(restore)
    }

    #[cfg(unix)]
    fn signals(events: Sender<Event>) -> io::Result<Self> {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
        let mut signals = signal_hook::iterator::Signals::new([SIGINT, SIGTERM, SIGHUP, SIGQUIT])?;
        let handle = signals.handle();
        let flag = Arc::new(AtomicI32::new(0));
        let raised = flag.clone();
        thread::spawn(move || {
            for signal in signals.forever() {
                let _ = raised.compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst);
                if events.send(Event::Wake).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            signal: flag,
            unicode_pushed: false,
            signals: handle,
        })
    }

    #[cfg(not(unix))]
    fn signals(events: Sender<Event>) -> io::Result<Self> {
        let flag = Arc::new(AtomicI32::new(0));
        let raised = flag.clone();
        ctrlc::set_handler(move || {
            let _ = raised.compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst);
            let _ = events.send(Event::Wake);
        })
        .map_err(io::Error::other)?;
        Ok(Self {
            signal: flag,
            unicode_pushed: false,
        })
    }
}

impl Drop for Restore {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = out.write_all(restore_sequence(self.unicode_pushed).as_bytes());
        let _ = out.flush();
        // Ends the signal thread, which unregisters the handlers.
        #[cfg(unix)]
        self.signals.close();
    }
}

/// Ends a synchronized update that may have been cut off, shows the
/// cursor and pops the Unicode version if it was pushed.
fn restore_sequence(unicode_pushed: bool) -> String {
    // Cancel an incomplete escape/OSC and close hyperlinks before emitting
    // cursor controls, including when output failed halfway through a frame.
    let mut sequence = String::from("\x18\x1b\\\x1b]8;;\x1b\\\x1b[0m\x1b[?2026l\x1b[?25h");
    if unicode_pushed {
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
            size: Default::default(),
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
