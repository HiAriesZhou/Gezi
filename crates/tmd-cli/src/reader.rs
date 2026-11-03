//! Static Markdown in the primary terminal screen. Only visible lines are
//! painted; the complete layout is kept in memory for wheel/keyboard scrolling.
use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crossterm::{cursor, event, terminal};
use event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use tmd_core::{Line, LineSize, RenderOptions, Theme, render_with_options, truncate};

use crate::cli::{Cli, InputFormat, ReaderMode};
use crate::{ansi, large_headings, reader_native};

const RESET: &str = "\x18\x1b\\\x1b]8;;\x1b\\\x1b[0m\x1b#5";

pub fn eligible(cli: &Cli) -> bool {
    cfg!(unix)
        && cli.file.is_some()
        && cli.from == InputFormat::Text
        && !cli.ansi
        && io::stdin().is_terminal()
        && io::stdout().is_terminal()
        && std::env::var("TERM").is_ok_and(|term| term != "dumb")
        && terminal::size().is_ok_and(|(cols, rows)| cols >= 4 && rows >= 4)
}

fn layout(cli: &Cli, md: &str, theme: &Theme, size: (u16, u16)) -> Vec<Line> {
    let width = cli.width.unwrap_or(size.0).min(size.0).max(1);
    let large = size.1 >= 4 && large_headings::enabled(cli, true, Some(size), width);
    render_with_options(
        md,
        width,
        theme,
        RenderOptions {
            large_headings: large,
        },
    )
}

fn header() -> String {
    std::env::args_os()
        .enumerate()
        .map(|(i, arg)| {
            let arg = if i == 0 {
                std::ffi::OsString::from("tmd")
            } else {
                arg
            };
            let text: String = arg
                .to_string_lossy()
                .chars()
                .filter(|ch| !ch.is_control())
                .collect();
            if text.contains(char::is_whitespace) {
                format!("{text:?}")
            } else {
                text
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn visible_end(lines: &[Line], start: usize, height: usize) -> usize {
    let mut used = 0;
    let mut end = start;
    for line in lines.iter().skip(start) {
        if used + line.physical_height() > height {
            break;
        }
        used += line.physical_height();
        end += 1;
    }
    end
}

fn last_start(lines: &[Line], height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    let mut start = lines.len();
    let mut used = 0;
    while start > 0 && used + lines[start - 1].physical_height() <= height {
        start -= 1;
        used += lines[start].physical_height();
    }
    start
}

fn paint(
    lines: &[Line],
    start: usize,
    size: (u16, u16),
    title: &str,
    notice: &str,
) -> (String, usize) {
    let (cols, rows) = size;
    let mut out = String::from("\x1b[?2026h");
    // Erasing alone does not reset DEC line attributes from an earlier frame.
    for row in 0..rows {
        out.push_str(&format!("\x1b[{};1H\x1b#5\x1b[2K", row + 1));
    }
    let body = rows.saturating_sub(2) as usize;
    let end = visible_end(lines, start, body);
    if rows >= 3 {
        out.push_str("\x1b[1;1H");
        out.push_str(truncate(title, cols.saturating_sub(1) as usize));
        let mut row = 2;
        for line in &lines[start..end] {
            let mut text = String::new();
            ansi::write_line(&mut text, line, true);
            match line.size {
                LineSize::Normal => {
                    out.push_str(&format!("\x1b[{row};1H{text}"));
                    row += 1;
                }
                LineSize::Double => {
                    out.push_str(&format!(
                        "\x1b[{row};1H\x1b#3{text}\x1b[{};1H\x1b#4{text}",
                        row + 1
                    ));
                    row += 2;
                }
            }
        }
    }
    let footer = if rows < 3 || cols < 4 {
        "q 退出 · 窗口过小".into()
    } else {
        let position = if lines.is_empty() {
            "空文档"
        } else if start == 0 {
            "开头"
        } else if end == lines.len() {
            "末尾"
        } else {
            "阅读中"
        };
        format!("q/Esc 返回 · {position}{notice} · 滚轮/↑↓ · Home/End")
    };
    if rows > 0 {
        out.push_str(&format!(
            "\x1b[{rows};1H\x1b#5\x1b[0m{}",
            truncate(&footer, cols.saturating_sub(1) as usize)
        ));
    }
    out.push_str("\x1b[?2026l");
    (out, end)
}

fn checked_write(
    out: &mut impl Write,
    mut bytes: &[u8],
    cancelled: &AtomicUsize,
) -> io::Result<()> {
    while !bytes.is_empty() {
        if cancelled.load(Ordering::SeqCst) != 0 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        match out.write(bytes) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => bytes = &bytes[n..],
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
    out.flush()
}

#[cfg(unix)]
struct Session {
    cancelled: Arc<AtomicUsize>,
    hooks: Vec<signal_hook::SigId>,
    original: nix::sys::termios::Termios,
    raw: bool,
    painted: bool,
    incomplete: bool,
    rows: u16,
    exit_row: u16,
}

#[cfg(unix)]
impl Session {
    fn start(rows: u16) -> io::Result<Self> {
        let mut session = Self {
            cancelled: Arc::new(AtomicUsize::new(0)),
            hooks: Vec::new(),
            original: nix::sys::termios::tcgetattr(io::stdin())?,
            raw: false,
            painted: false,
            incomplete: false,
            rows,
            exit_row: rows.saturating_sub(1),
        };
        for signal in [
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
            signal_hook::consts::SIGQUIT,
        ] {
            session.hooks.push(signal_hook::flag::register_usize(
                signal,
                session.cancelled.clone(),
                signal as usize,
            )?);
        }
        terminal::enable_raw_mode()?;
        session.raw = true;
        Ok(session)
    }

    fn restore_input(&mut self) -> io::Result<()> {
        if self.raw {
            let result = terminal::disable_raw_mode();
            nix::sys::termios::tcsetattr(
                io::stdin(),
                nix::sys::termios::SetArg::TCSANOW,
                &self.original,
            )?;
            self.raw = false;
            result?;
        }
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for Session {
    fn drop(&mut self) {
        let mut out = io::stdout().lock();
        let _ = write!(out, "{RESET}\x1b[?2026l");
        if self.painted {
            if self.incomplete {
                for row in 1..=self.rows {
                    let _ = write!(out, "\x1b[{row};1H\x1b#5\x1b[2K");
                }
            }
            let _ = write!(out, "\x1b[{};1H\x1b#5\x1b[2K", self.rows.max(1));
            let _ = write!(out, "\x1b[{};1H\x1b#5\r", self.exit_row.saturating_add(1));
            let _ = crossterm::execute!(out, event::DisableMouseCapture, cursor::Show);
        }
        let _ = out.flush();
        drop(out);
        let _ = self.restore_input();
        for hook in self.hooks.drain(..) {
            signal_hook::low_level::unregister(hook);
        }
    }
}

#[cfg(unix)]
pub fn run(cli: &Cli, md: &str, theme: &Theme, mode: ReaderMode) -> io::Result<u8> {
    let mut size = terminal::size()?;
    let mut lines = layout(cli, md, theme, size);
    let title = header();
    let mut session = Session::start(size.1)?;
    // This is screen scrolling, not an alternate buffer or a history purge.
    // Using the live cursor row also handles commands started near the bottom.
    let (_, row) = cursor::position()?;
    if row > 0 {
        crossterm::execute!(io::stdout(), terminal::ScrollUp(row))?;
    }
    crossterm::execute!(io::stdout(), cursor::MoveTo(0, 0))?;
    let mut queued = VecDeque::new();
    let token = reader_native::token();
    let mut notice = String::new();
    if mode == ReaderMode::Native {
        let capacity = if reader_native::direct_kaku() {
            reader_native::probe(&mut io::stdout().lock(), &token, &mut queued)?
        } else {
            None
        };
        let required: usize = lines
            .iter()
            .map(Line::physical_height)
            .sum::<usize>()
            .saturating_add(2);
        if capacity.is_some_and(|capacity| capacity >= required) && queued.is_empty() {
            session.restore_input()?;
            let mut out = io::stdout().lock();
            writeln!(
                out,
                "{}",
                truncate(&title, size.0.saturating_sub(1) as usize)
            )?;
            out.flush()?;
            drop(out);
            let result = if lines.iter().any(|line| line.size == LineSize::Double) {
                large_headings::write(&lines)
            } else {
                checked_write(
                    &mut io::stdout().lock(),
                    ansi::to_ansi(&lines, true).as_bytes(),
                    &session.cancelled,
                )
            };
            let signal = session.cancelled.load(Ordering::SeqCst);
            if signal != 0 {
                return Ok((128 + signal) as u8);
            }
            result?;
            reader_native::notify(&mut io::stdout().lock(), "show", &token)?;
            return Ok(0);
        }
        notice = " · 原生定位不可用，使用内置阅读".into();
    }
    let result: io::Result<()> = (|| {
        session.painted = true;
        crossterm::execute!(
            io::stdout(),
            cursor::MoveTo(0, 0),
            cursor::Hide,
            event::EnableMouseCapture
        )?;
        let mut start = 0;
        loop {
            if session.cancelled.load(Ordering::SeqCst) != 0 {
                break;
            }
            session.rows = size.1;
            session.exit_row = size.1.saturating_sub(1);
            session.incomplete = true;
            let (frame, end) = paint(&lines, start, size, &title, &notice);
            checked_write(
                &mut io::stdout().lock(),
                frame.as_bytes(),
                &session.cancelled,
            )?;
            session.incomplete = false;
            let short = lines.iter().map(Line::physical_height).sum::<usize>()
                <= size.1.saturating_sub(2) as usize;
            if short && queued.is_empty() && event::poll(Duration::ZERO)? {
                queued.push_back(event::read()?);
            }
            if short && queued.is_empty() {
                session.exit_row = (1 + lines.iter().map(Line::physical_height).sum::<usize>())
                    .min(size.1.saturating_sub(1) as usize)
                    as u16;
                break;
            }
            let input = loop {
                if let Some(event) = queued.pop_front() {
                    break event;
                }
                if session.cancelled.load(Ordering::SeqCst) != 0 {
                    return Ok(());
                }
                if event::poll(Duration::from_millis(100))? {
                    break event::read()?;
                }
            };
            let max = last_start(&lines, size.1.saturating_sub(2) as usize);
            match input {
                Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        session.cancelled.store(2, Ordering::SeqCst);
                        break;
                    }
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Up | KeyCode::Char('k') => start = start.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => start = start.saturating_add(1).min(max),
                    KeyCode::Home => start = 0,
                    KeyCode::End => start = max,
                    KeyCode::PageDown | KeyCode::Char(' ') => {
                        start = end.saturating_sub(1).max(start + 1).min(max)
                    }
                    KeyCode::PageUp | KeyCode::Char('b') => {
                        start = start.saturating_sub(size.1.saturating_sub(3) as usize)
                    }
                    _ => continue,
                },
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => start = start.saturating_sub(3),
                    MouseEventKind::ScrollDown => start = start.saturating_add(3).min(max),
                    _ => continue,
                },
                Event::Resize(cols, rows) => {
                    let anchor = lines.get(start).map_or(0, |line| line.src.start);
                    size = (cols, rows);
                    lines = layout(cli, md, theme, size);
                    start = lines
                        .iter()
                        .position(|line| line.src.start >= anchor)
                        .unwrap_or(0)
                        .min(last_start(&lines, rows.saturating_sub(2) as usize));
                }
                _ => continue,
            }
        }
        Ok(())
    })();
    let signal = session.cancelled.load(Ordering::SeqCst);
    if signal != 0 {
        return Ok((128 + signal) as u8);
    }
    result?;
    Ok(0)
}

#[cfg(not(unix))]
pub fn run(_: &Cli, _: &str, _: &Theme, _: ReaderMode) -> io::Result<u8> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmd_core::{Span, Style};
    fn row(size: LineSize) -> Line {
        Line {
            spans: vec![Span::new("中 é 👩‍💻", Style::default())],
            src: 0..20,
            size,
        }
    }
    #[test]
    fn double_rows_are_never_split_by_a_viewport() {
        let lines = vec![
            row(LineSize::Normal),
            row(LineSize::Double),
            row(LineSize::Normal),
        ];
        assert_eq!(visible_end(&lines, 0, 2), 1);
        assert_eq!(visible_end(&lines, 1, 2), 2);
        assert_eq!(last_start(&lines, 2), 2);
        let (frame, end) = paint(&lines, 0, (40, 4), "tmd test.md", "");
        assert_eq!(end, 1);
        assert!(!frame.contains("\x1b#3"));
        let (frame, end) = paint(&lines, 1, (40, 4), "tmd test.md", "");
        assert_eq!(end, 2);
        assert_eq!(frame.matches("\x1b#3").count(), 1);
        assert_eq!(frame.matches("\x1b#4").count(), 1);
        assert!(!frame.contains("?1049"));
        assert!(!frame.contains("\x1b[3J"));
    }
    #[test]
    fn resize_can_show_a_tiny_window_without_leaking_body_rows() {
        let (frame, end) = paint(&[row(LineSize::Double)], 0, (4, 1), "header", "");
        assert_eq!(end, 0);
        assert!(!frame.contains("\x1b#3"));
        assert!(!frame.contains("header"));
    }
}
