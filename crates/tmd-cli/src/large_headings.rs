//! Opt-in DEC double-height output for static Markdown files.

use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use tmd_core::{Line, LineSize};

use crate::ansi;
use crate::cli::{Cli, InputFormat};
use crate::terminal_io::{RESET, is_direct_kaku, write_all_cancelled};

static INTERRUPTED: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
const NORMAL: &str = "\x1b#5";

pub fn enabled(cli: &Cli, tty: bool, size: Option<(u16, u16)>, width: u16) -> bool {
    eligible(
        cli,
        tty,
        size,
        width,
        std::env::var("TERM_PROGRAM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
        std::env::var_os("TMUX").is_some() || std::env::var_os("STY").is_some(),
    )
}

fn eligible(
    cli: &Cli,
    tty: bool,
    size: Option<(u16, u16)>,
    width: u16,
    program: Option<&str>,
    term: Option<&str>,
    multiplexed: bool,
) -> bool {
    cfg!(unix)
        && cli.large_headings
        && cli.file.is_some()
        && cli.from == InputFormat::Text
        && !cli.ansi
        && tty
        && is_direct_kaku(program, term, multiplexed)
        && size.is_some_and(|(cols, rows)| cols >= 4 && rows >= 3 && width <= cols)
        && width >= 4
}

/// The signal handler only requests cancellation. The output thread owns all
/// terminal writes, so cleanup cannot race an in-flight top/bottom row.
#[cfg(unix)]
pub fn write(lines: &[Line]) -> io::Result<()> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};

    let interrupted = INTERRUPTED.get_or_init(|| Arc::new(AtomicUsize::new(0)));
    // Register before changing tty settings. All handled exits use the same
    // output-thread cleanup; SIGKILL cannot run cleanup in any program.
    for signal in [SIGINT, SIGHUP, SIGQUIT, SIGTERM] {
        signal_hook::flag::register_usize(signal, interrupted.clone(), signal as usize)?;
    }
    let mut settings = TtySettings::start()?;
    let result = write_to(io::stdout().lock(), lines, interrupted);
    let restored = settings.restore();
    result.and(restored)?;
    if interrupted.load(Ordering::SeqCst) != 0 {
        return Err(io::ErrorKind::Interrupted.into());
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn write(_lines: &[Line]) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

pub fn interrupted_exit_code() -> u8 {
    let signal = INTERRUPTED
        .get()
        .map_or(2, |signal| signal.load(Ordering::SeqCst));
    128 + signal as u8
}

#[cfg(unix)]
struct TtySettings {
    original: nix::sys::termios::Termios,
    restored: bool,
}

#[cfg(unix)]
impl TtySettings {
    fn start() -> io::Result<Self> {
        use nix::sys::termios::{self, LocalFlags, SetArg};
        let original = termios::tcgetattr(io::stdout())?;
        let mut protected = original.clone();
        // VINTR must not drop queued half-lines or echo into an escape.
        // Keep canonical input, ISIG and all output processing as configured.
        protected.local_flags.insert(LocalFlags::NOFLSH);
        protected
            .local_flags
            .remove(LocalFlags::ECHO | LocalFlags::ECHONL);
        termios::tcsetattr(io::stdout(), SetArg::TCSANOW, &protected)?;
        Ok(Self {
            original,
            restored: false,
        })
    }

    fn restore(&mut self) -> io::Result<()> {
        use nix::sys::termios::{self, FlushArg, SetArg};
        if !self.restored {
            let flushed = if INTERRUPTED
                .get()
                .is_some_and(|signal| signal.load(Ordering::SeqCst) != 0)
            {
                // Match the usual interrupt's input-queue discard while
                // preserving the output queue until DEC cleanup is written.
                termios::tcflush(io::stdout(), FlushArg::TCIFLUSH)
            } else {
                Ok(())
            };
            // Always attempt attribute restoration, even if queue cleanup
            // failed (for example, while the terminal is disconnecting).
            termios::tcsetattr(io::stdout(), SetArg::TCSANOW, &self.original)?;
            self.restored = true;
            flushed?;
        }
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for TtySettings {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[derive(Clone, Copy, Default)]
enum Pending {
    #[default]
    Clean,
    NormalRow,
    Top,
    Bottom,
}

struct Output<'a, W: Write> {
    writer: W,
    pending: Pending,
    cancelled: &'a AtomicUsize,
}

impl<W: Write> Output<'_, W> {
    fn bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        write_all_cancelled(&mut self.writer, bytes, self.cancelled)
    }

    fn row(&mut self, text: &str, pending: Pending, next: Pending) -> io::Result<()> {
        self.pending = pending;
        self.bytes(text.as_bytes())?;
        // LF is a separate one-byte write: a short write cannot leave the
        // cleanup state unsure which physical row currently holds the cursor.
        self.bytes(b"\r")?;
        self.bytes(b"\n")?;
        self.pending = next;
        Ok(())
    }
}

impl<W: Write> Drop for Output<'_, W> {
    fn drop(&mut self) {
        let tail = match self.pending {
            Pending::Clean => "",
            Pending::NormalRow | Pending::Top => "\r\x1b[2K\r\n\x1b#5",
            // Bottom may not have started yet. Remove the unpaired top too;
            // the minimum viewport height ensures it is still addressable.
            Pending::Bottom => "\r\x1b[2K\x1b[1F\x1b#5\x1b[2K\x1b[1E\r\n\x1b#5",
        };
        // Best effort after I/O failure; a disconnected terminal cannot be
        // restored. Do not switch to a potentially different controlling tty.
        let _ = self.writer.write_all(format!("{RESET}{tail}").as_bytes());
        let _ = self.writer.flush();
    }
}

fn write_to(writer: impl Write, lines: &[Line], cancelled: &AtomicUsize) -> io::Result<()> {
    let mut output = Output {
        writer,
        pending: Pending::Clean,
        cancelled,
    };
    for line in lines {
        let mut text = String::new();
        ansi::write_line(&mut text, line, true);
        match line.size {
            LineSize::Normal => output.row(
                &format!("{NORMAL}\r\x1b[2K{text}"),
                Pending::NormalRow,
                Pending::Clean,
            )?,
            LineSize::Double => {
                output.row(
                    &format!("{NORMAL}\r\x1b[2K\x1b#3{text}"),
                    Pending::Top,
                    Pending::Bottom,
                )?;
                output.row(
                    &format!("{NORMAL}\r\x1b[2K\x1b#4{text}"),
                    Pending::Bottom,
                    Pending::Clean,
                )?;
            }
        }
    }
    output.writer.flush()?;
    if cancelled.load(Ordering::SeqCst) != 0 {
        return Err(io::ErrorKind::Interrupted.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use tmd_core::{Span, Style};

    use super::*;

    #[test]
    fn capability_gate_is_strict_and_runs_before_layout() {
        let cli = Cli::parse_from(["tmd", "--large-headings", "x.md"]);
        let allowed = |tty, size, width, program, term, mux| {
            eligible(&cli, tty, size, width, program, term, mux)
        };
        assert!(allowed(
            true,
            Some((80, 24)),
            80,
            Some("Kaku"),
            Some("kaku"),
            false
        ));
        for (tty, size, width, program, term, mux) in [
            (false, Some((80, 24)), 80, Some("Kaku"), Some("kaku"), false),
            (true, None, 80, Some("Kaku"), Some("kaku"), false),
            (true, Some((3, 24)), 3, Some("Kaku"), Some("kaku"), false),
            (true, Some((80, 2)), 80, Some("Kaku"), Some("kaku"), false),
            (true, Some((80, 24)), 81, Some("Kaku"), Some("kaku"), false),
            (true, Some((80, 24)), 3, Some("Kaku"), Some("kaku"), false),
            (
                true,
                Some((80, 24)),
                80,
                Some("WezTerm"),
                Some("xterm"),
                false,
            ),
            (true, Some((80, 24)), 80, None, Some("xterm"), false),
            (true, Some((80, 24)), 80, Some("Kaku"), Some("dumb"), false),
            (
                true,
                Some((80, 24)),
                80,
                Some("Kaku"),
                Some("screen-256color"),
                false,
            ),
            (true, Some((80, 24)), 80, Some("Kaku"), Some("kaku"), true),
        ] {
            assert!(!allowed(tty, size, width, program, term, mux));
        }
        for args in [
            vec!["tmd", "x.md"],
            vec!["tmd", "--large-headings"],
            vec!["tmd", "--large-headings", "--ansi", "x.md"],
            vec!["tmd", "--large-headings", "--from", "claude-json", "x.md"],
        ] {
            assert!(!eligible(
                &Cli::parse_from(args),
                true,
                Some((80, 24)),
                80,
                Some("Kaku"),
                Some("kaku"),
                false
            ));
        }
    }

    fn heading() -> Line {
        Line {
            spans: vec![Span::new("H1", Style::default())],
            src: 0..4,
            size: LineSize::Double,
        }
    }

    #[test]
    fn emits_matching_halves_and_normal_rows_without_autowrap() {
        let mut bytes = Vec::new();
        let mut body = heading();
        body.size = LineSize::Normal;
        write_to(&mut bytes, &[heading(), body], &AtomicUsize::new(0)).unwrap();
        let out = String::from_utf8(bytes).unwrap();
        assert!(out.starts_with(
            "\x1b#5\r\x1b[2K\x1b#3H1\r\n\x1b#5\r\x1b[2K\x1b#4H1\r\n\x1b#5\r\x1b[2KH1\r\n"
        ));
        assert!(out.ends_with(RESET));
        // Ordinary ANSI output never emits size controls or a second copy.
        assert_eq!(ansi::to_ansi(&[heading()], false), "H1\n");
    }

    struct Fault<'a> {
        bytes: Vec<u8>,
        at: usize,
        failed: bool,
        cancel: Option<&'a AtomicUsize>,
    }

    impl Write for Fault<'_> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.bytes.len() == self.at && !self.failed {
                self.failed = true;
                if let Some(cancel) = self.cancel {
                    cancel.store(2, Ordering::SeqCst);
                    return Err(io::ErrorKind::Interrupted.into());
                }
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            let count = if self.failed {
                buf.len()
            } else {
                buf.len().min(self.at - self.bytes.len())
            };
            self.bytes.extend_from_slice(&buf[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn every_partial_write_and_cancellation_restores_line_size() {
        let cancel = AtomicUsize::new(0);
        let mut full = Vec::new();
        write_to(&mut full, &[heading()], &cancel).unwrap();
        let content_len = full.len() - RESET.len();
        let top_end = full.iter().position(|&b| b == b'\n').unwrap() + 1;
        for interrupted in [false, true] {
            for at in 0..content_len {
                cancel.store(0, Ordering::SeqCst);
                let mut writer = Fault {
                    bytes: Vec::new(),
                    at,
                    failed: false,
                    cancel: interrupted.then_some(&cancel),
                };
                let error = write_to(&mut writer, &[heading()], &cancel).unwrap_err();
                assert_eq!(
                    error.kind(),
                    if interrupted {
                        io::ErrorKind::Interrupted
                    } else {
                        io::ErrorKind::BrokenPipe
                    }
                );
                let cleanup = &writer.bytes[at..];
                assert!(cleanup.starts_with(RESET.as_bytes()), "offset {at}");
                assert!(cleanup.ends_with(NORMAL.as_bytes()), "offset {at}");
                if at >= top_end {
                    assert!(
                        cleanup.windows(4).any(|bytes| bytes == b"\x1b[1F"),
                        "unpaired top at {at}"
                    );
                }
            }
        }
    }
}
