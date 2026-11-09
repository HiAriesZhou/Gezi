//! Static Markdown in the primary terminal screen. Only visible lines are
//! painted; the complete layout is kept in memory for wheel/keyboard scrolling.
use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crossterm::{cursor, event, terminal};
use event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};
use tmd_core::{Document, Line, LineSize, RenderOptions, Theme, truncate};

use crate::cli::{Cli, InputFormat, ReaderMode};
use crate::clipboard::{self, CopyError};
use crate::reader_copy::CodePicker;
use crate::reader_outline::Outline;
use crate::reader_search::Search;
use crate::reader_ui::{self, Notice, Pane, View};
use crate::terminal_io::{RESET, direct_kaku, write_all_cancelled};
use crate::{ansi, large_headings, reader_native};

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

fn layout(cli: &Cli, document: &Document, size: (u16, u16)) -> Vec<Line> {
    reader_layout(cli, document, size, false)
}

fn sidebar_width(cols: u16, open: bool) -> Option<u16> {
    (open && cols >= 80).then(|| (cols / 4).clamp(24, 36))
}

fn reader_layout(cli: &Cli, document: &Document, size: (u16, u16), open: bool) -> Vec<Line> {
    reader_image_layout(cli, document, size, open, None)
}

fn reader_image_layout(
    cli: &Cli,
    document: &Document,
    size: (u16, u16),
    open: bool,
    images: Option<&tmd_core::ImageLayout<'_>>,
) -> Vec<Line> {
    let sidebar = sidebar_width(size.0, open);
    let size = (size.0.saturating_sub(sidebar.map_or(0, |w| w + 2)), size.1);
    let width = cli.width.unwrap_or(size.0).min(size.0).max(1);
    let large =
        sidebar.is_none() && size.1 >= 4 && large_headings::enabled(cli, true, Some(size), width);
    let options = RenderOptions {
        large_headings: large,
    };
    match images {
        Some(images) => document.layout_with_images(width, options, images),
        None => document.layout(width, options),
    }
}

struct LayoutCache {
    key: ((u16, u16), Option<u16>),
    alternate: Option<(Option<u16>, Vec<Line>)>,
}

impl LayoutCache {
    fn invalidate(&mut self) {
        self.key = ((0, 0), None);
        self.alternate = None;
    }
    fn new(size: (u16, u16)) -> Self {
        Self {
            key: (size, None),
            alternate: None,
        }
    }

    fn update(
        &mut self,
        lines: &mut Vec<Line>,
        size: (u16, u16),
        sidebar: Option<u16>,
        build: impl FnOnce() -> Vec<Line>,
    ) -> bool {
        if self.key == (size, sidebar) {
            return false;
        }
        if self.key.0 != size {
            self.alternate = None;
        }
        let next = self
            .alternate
            .take()
            .filter(|(width, _)| *width == sidebar)
            .map(|(_, cached)| cached)
            .unwrap_or_else(build);
        let previous = std::mem::replace(lines, next);
        if self.key.0 == size {
            self.alternate = Some((self.key.1, previous));
        }
        self.key = (size, sidebar);
        true
    }
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
    let mut start = lines.len();
    let mut used = 0;
    while start > 0 && used + lines[start - 1].physical_height() <= height {
        start -= 1;
        used += lines[start].physical_height();
    }
    start.min(lines.len().saturating_sub(1))
}

#[derive(Default)]
struct Chrome {
    title: String,
    details: Vec<Line>,
    footer: String,
}

impl Chrome {
    fn height(&self, rows: u16) -> usize {
        rows.saturating_sub(2 + self.details.len() as u16) as usize
    }
}

fn paint(
    lines: &[Line],
    start: usize,
    size: (u16, u16),
    chrome: &Chrome,
    highlight: Option<usize>,
) -> (String, usize) {
    let (cols, rows) = size;
    let mut out = String::from("\x1b[?2026h");
    // Erasing alone does not reset DEC line attributes from an earlier frame.
    for row in 0..rows {
        out.push_str(&format!("\x1b[{};1H\x1b#5\x1b[2K", row + 1));
    }
    let body = chrome.height(rows);
    let end = visible_end(lines, start, body);
    if rows >= 3 {
        out.push_str("\x1b[1;1H");
        out.push_str(truncate(&chrome.title, cols.saturating_sub(1) as usize));
        let mut row = 2;
        for detail in &chrome.details {
            out.push_str(&format!(
                "\x1b[{row};1H{}",
                truncate(&detail.plain_text(), cols.saturating_sub(1) as usize)
            ));
            row += 1;
        }
        for (index, line) in lines[start..end].iter().enumerate() {
            let mut text = String::new();
            if highlight == Some(start + index) {
                let mut marked = line.clone();
                for span in &mut marked.spans {
                    span.style.bold = true;
                    span.style.underline = true;
                }
                ansi::write_line(&mut text, &marked, true);
            } else {
                ansi::write_line(&mut text, line, true);
            }
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
    if rows > 0 {
        out.push_str(&format!(
            "\x1b[{rows};1H\x1b#5\x1b[0m{}",
            truncate(&chrome.footer, cols.saturating_sub(1) as usize)
        ));
    }
    out.push_str("\x1b[?2026l");
    (out, end)
}

/// Clip styled text at the pane boundary before emitting ANSI. Never let an
/// oversized code/table row overwrite the divider or wrap the terminal.
fn pane_line(out: &mut String, line: &Line, width: usize, selected: bool) {
    let mut clipped = line.clone();
    let mut remaining = width;
    for span in &mut clipped.spans {
        let part = truncate(&span.text, remaining).to_owned();
        remaining = remaining.saturating_sub(tmd_core::text_width(&part));
        if part.len() != span.text.len() {
            remaining = 0;
        }
        span.text = part;
        if selected {
            span.style.bold = true;
            span.style.underline = true;
        }
    }
    ansi::write_line(out, &clipped, true);
}

struct SidebarFrame {
    width: u16,
    title: String,
    lines: Vec<Line>,
    top: usize,
    focused: bool,
}

fn paint_split(
    lines: &[Line],
    start: usize,
    size: (u16, u16),
    chrome: &Chrome,
    highlight: Option<usize>,
    sidebar: &SidebarFrame,
) -> (String, usize) {
    // Reuse the full-frame clearing/footer contract; both panes use normal rows.
    let (mut out, _) = paint(
        &[],
        0,
        size,
        &Chrome {
            footer: chrome.footer.clone(),
            ..Chrome::default()
        },
        None,
    );
    out.truncate(out.len() - "\x1b[?2026l".len());
    let height = size.1.saturating_sub(2) as usize;
    let end = visible_end(lines, start, height);
    if size.1 >= 3 {
        let right = sidebar.width + 2;
        let body_width = size.0.saturating_sub(right) as usize;
        for row in 1..size.1 {
            out.push_str(&format!("\x1b[{row};{}H│", sidebar.width + 1));
        }
        out.push_str("\x1b[1;1H");
        out.push_str(if sidebar.focused {
            "\x1b[1m"
        } else {
            "\x1b[2m"
        });
        out.push_str(truncate(
            &sidebar.title,
            sidebar.width.saturating_sub(1) as usize,
        ));
        out.push_str("\x1b[0m");
        out.push_str(&format!("\x1b[1;{right}H"));
        out.push_str(if sidebar.focused {
            "\x1b[2m"
        } else {
            "\x1b[1m"
        });
        out.push_str(truncate(&chrome.title, body_width));
        out.push_str("\x1b[0m");
        for (index, line) in sidebar
            .lines
            .iter()
            .skip(sidebar.top)
            .take(height)
            .enumerate()
        {
            let row = index + 2;
            out.push_str(&format!("\x1b[{row};1H"));
            pane_line(
                &mut out,
                line,
                sidebar.width.saturating_sub(1) as usize,
                false,
            );
        }
        for (index, line) in lines.iter().enumerate().take(end).skip(start) {
            out.push_str(&format!("\x1b[{};{right}H", 2 + index - start));
            pane_line(&mut out, line, body_width, highlight == Some(index));
        }
    }
    out.push_str("\x1b[?2026l");
    (out, end)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Exit,
    Interrupt,
    Help,
    Up,
    Down,
    Top,
    Bottom,
    PageUp,
    PageDown,
    Search,
    Next,
    Previous,
    Copy,
    Outline,
    Confirm,
    Focus,
}

fn key_action(key: KeyEvent) -> Option<Action> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
        return Some(Action::Interrupt);
    }
    if key.modifiers.intersects(
        KeyModifiers::CONTROL
            | KeyModifiers::ALT
            | KeyModifiers::SUPER
            | KeyModifiers::HYPER
            | KeyModifiers::META,
    ) {
        return None;
    }
    Some(match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Exit,
        KeyCode::Char('?') => Action::Help,
        KeyCode::Char('/') => Action::Search,
        KeyCode::Char('c') => Action::Copy,
        KeyCode::Char('o') => Action::Outline,
        KeyCode::Enter => Action::Confirm,
        KeyCode::Tab | KeyCode::BackTab => Action::Focus,
        KeyCode::Char('n') => Action::Next,
        KeyCode::Char('N') => Action::Previous,
        KeyCode::Up | KeyCode::Char('k') => Action::Up,
        KeyCode::Down | KeyCode::Char('j') => Action::Down,
        KeyCode::Home => Action::Top,
        KeyCode::End => Action::Bottom,
        KeyCode::PageDown | KeyCode::Char(' ') => Action::PageDown,
        KeyCode::PageUp | KeyCode::Char('b') => Action::PageUp,
        _ => return None,
    })
}

fn list_movement(action: Option<Action>, event: &Event, height: usize) -> Option<isize> {
    match action {
        Some(Action::Up) => Some(-1),
        Some(Action::Down) => Some(1),
        Some(Action::Top) => Some(isize::MIN),
        Some(Action::Bottom) => Some(isize::MAX),
        Some(Action::PageUp) => Some(-(height.max(1) as isize)),
        Some(Action::PageDown) => Some(height.max(1) as isize),
        _ => match event {
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => Some(-3),
                MouseEventKind::ScrollDown => Some(3),
                _ => None,
            },
            _ => None,
        },
    }
}

fn scroll_position(
    position: usize,
    end: usize,
    max: usize,
    height: usize,
    action: Option<Action>,
    event: &Event,
) -> usize {
    match action {
        Some(Action::PageDown) => end.saturating_sub(1).max(position + 1).min(max),
        Some(Action::PageUp) => position.saturating_sub(height.saturating_sub(1).max(1)),
        _ => list_movement(action, event, height).map_or(position, |amount| {
            position.saturating_add_signed(amount).min(max)
        }),
    }
}

fn help_lines(width: u16, pane: Pane, fallback: &str, copy_error: &str) -> Vec<Line> {
    let common = [
        "↑↓ / j k / 滚轮：移动；PageUp/b、PageDown/空格：翻页；Home/End：首尾",
        "? / Esc / q：关闭帮助，返回打开帮助前的界面",
        "Ctrl-C：中断，恢复终端并返回命令行",
        "Shift + 拖选：终端原生选择（Kaku）",
    ];
    let specific: &[&str] = match pane {
        Pane::Reading => &[
            "阅读帮助",
            "/：搜索正文（区分大小写，Enter 确认）；n/N：下一处/上一处匹配",
            "搜索输入中 Esc 取消；Ctrl-U 清空；q、o、c 等作为查询文字",
            "o：开关标题目录；Tab：侧栏与正文切换焦点；c：代码块；?：帮助",
            "阅读时 q 直接退出；Esc 依次清除搜索、关闭侧栏、退出",
        ],
        Pane::Outline => &[
            "目录帮助",
            "背景为选中项；▎ 为当前阅读章节；L 为源码行号",
            "Enter：跳转并保留目录焦点，可连续选择跳转；Tab：切换到正文",
            "窄窗口中 Enter 跳转后关闭目录",
            "Esc / q / o：关闭目录；滚轮只移动指针所在区域",
            "80 列起显示侧栏并使用普通标题，窄窗口使用整页目录",
            "选择不会移动正文。重复和空标题按各自源码位置跳转",
            "目录跳转保留搜索；n/N 仍从原先选中的匹配继续",
        ],
        Pane::Copy => &[
            "代码块帮助",
            "Enter：复制选中代码；Esc / q / c：取消，回到原阅读位置",
            "仅 Enter 会尝试复制；成功返回正文；失败或空代码保留列表",
        ],
    };
    specific
        .iter()
        .chain(common.iter())
        .copied()
        .chain([fallback, copy_error].into_iter().filter(|s| !s.is_empty()))
        .flat_map(|text| reader_ui::wrapped(text, width.saturating_sub(1) as usize, usize::MAX))
        .collect()
}

/// Keep an anchor across zero-height/very narrow layouts instead of repeatedly
/// capturing the clamped temporary viewport during a resize sequence.
#[derive(Clone, Default)]
struct Anchor {
    text: Option<usize>,
    source: usize,
    image: Option<(usize, u16, u16)>,
}

impl Anchor {
    fn at(lines: &[Line], start: usize) -> Self {
        let line = lines.get(start);
        Self {
            text: line.and_then(|l| {
                l.spans
                    .iter()
                    .find_map(|s| s.text_ranges.first().map(|r| r.start))
            }),
            source: line.map_or(0, |l| l.src.start),
            image: line
                .and_then(|line| line.spans.iter().find_map(|span| span.image_cell))
                .map(|cell| (cell.index, cell.row, cell.rows)),
        }
    }

    fn resolve(&self, lines: &[Line], height: usize) -> usize {
        self.image
            .and_then(|(index, row, rows)| {
                lines.iter().position(|line| {
                    line.spans.iter().any(|span| {
                        span.image_cell.is_some_and(|cell| {
                            cell.index == index
                                && u32::from(cell.row)
                                    == u32::from(row) * u32::from(cell.rows)
                                        / u32::from(rows.max(1))
                        })
                    })
                })
            })
            .or_else(|| {
                self.text
                    .and_then(|text| Document::match_line(lines, &(text..text + 1)))
            })
            .or_else(|| lines.iter().position(|l| l.src.start >= self.source))
            .unwrap_or_else(|| lines.len().saturating_sub(1))
            .min(last_start(lines, height))
    }
}

fn section_source(document: &Document, lines: &[Line], start: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(start)
        .find(|(index, line)| {
            line.spans
                .iter()
                .any(|span| !span.text_ranges.is_empty() || span.code_block.is_some())
                || document.headings().iter().any(|heading| {
                    heading.source == line.src
                        && Document::heading_line(lines, heading) == Some(*index)
                })
        })
        .map(|(_, line)| line)
        .or_else(|| lines.get(start))
        .map_or(0, |line| line.src.start)
}

fn checked_write(out: &mut impl Write, bytes: &[u8], cancelled: &AtomicUsize) -> io::Result<()> {
    write_all_cancelled(out, bytes, cancelled)?;
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
            let _ = crossterm::execute!(
                out,
                event::DisableMouseCapture,
                event::DisableBracketedPaste,
                cursor::Show
            );
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
    run_with_copy(cli, md, theme, mode, clipboard::copy)
}

#[cfg(unix)]
fn run_with_copy(
    cli: &Cli,
    md: &str,
    theme: &Theme,
    mode: ReaderMode,
    mut copy: impl FnMut(&str) -> Result<(), CopyError>,
) -> io::Result<u8> {
    let mut size = terminal::size()?;
    let document = Document::new(md, theme.clone());
    let mut lines = layout(cli, &document, size);
    let content_height: usize = lines.iter().map(Line::physical_height).sum();
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
        let capacity = if direct_kaku() {
            reader_native::probe(&mut io::stdout().lock(), &token, &mut queued)?
        } else {
            None
        };
        let required = content_height.saturating_add(2);
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
    let mut images = crate::reader_images::Images::new(cli, &document);
    let result: io::Result<()> = (|| {
        session.painted = true;
        crossterm::execute!(
            io::stdout(),
            cursor::MoveTo(0, 0),
            cursor::Hide,
            event::EnableMouseCapture,
            event::EnableBracketedPaste
        )?;
        let mut start = 0;
        let mut anchor = Anchor::at(&lines, start);
        let mut search_anchor = anchor.clone();
        let mut view = View::Reading;
        let mut outline_open = false;
        let mut layouts = LayoutCache::new(size);
        if images.layout().is_some() {
            layouts.invalidate();
        }
        let mut help_start = 0;
        let mut search = Search::default();
        let mut picker = CodePicker::default();
        let mut outline = Outline::default();
        let fallback = notice;
        let mut notice = Notice::default();
        if !fallback.is_empty() {
            notice.show(fallback.trim_start_matches(" · "), Instant::now());
        }
        'reader: loop {
            if session.cancelled.load(Ordering::SeqCst) != 0 {
                break;
            }
            let width = sidebar_width(size.0, outline_open);
            let geometry_changed = images.refresh_geometry();
            if images.poll().1 || geometry_changed {
                layouts.invalidate();
            }
            if layouts.update(&mut lines, size, width, || {
                reader_image_layout(cli, &document, size, outline_open, images.layout().as_ref())
            }) {
                start = anchor.resolve(&lines, size.1.saturating_sub(2) as usize);
                search.saved_start =
                    search_anchor.resolve(&lines, size.1.saturating_sub(2) as usize);
            }
            // Narrow windows temporarily present the outline full-page without
            // forgetting which pane had focus in the split layout.
            let active_view = if outline_open && width.is_none() && view == View::Reading {
                View::Outline
            } else {
                view
            };
            let split = width
                .filter(|_| matches!(active_view, View::Reading | View::Search | View::Outline));
            if outline_open {
                let source = section_source(&document, &lines, start);
                outline.current = document
                    .headings()
                    .iter()
                    .rposition(|h| h.source.start <= source);
            }
            session.rows = size.1;
            session.exit_row = size.1.saturating_sub(1);
            session.incomplete = true;
            let mut chrome = Chrome::default();
            let mut state = String::new();
            let transient = notice.text(Instant::now());
            let overlay;
            let (display, position, highlight) = match active_view {
                View::Reading | View::Search => {
                    chrome.title = if view == View::Search {
                        reader_ui::footer(
                            View::Search,
                            "搜索",
                            false,
                            split.map_or(size.0, |w| size.0 - w - 1),
                        )
                    } else {
                        title.clone()
                    };
                    let end = visible_end(&lines, start, chrome.height(size.1));
                    let position = if start == 0 {
                        "开头"
                    } else if end == lines.len() {
                        "末尾"
                    } else {
                        "阅读中"
                    };
                    state = if search.query.is_empty() {
                        format!("阅读 · {position}")
                    } else {
                        search.notice().trim_start_matches(" · ").into()
                    };
                    if let Some(message) = transient {
                        state = message.into();
                    }
                    (
                        &lines,
                        start,
                        if view == View::Search {
                            None
                        } else {
                            search
                                .current()
                                .and_then(|m| Document::match_line(&lines, m))
                        },
                    )
                }
                View::Outline if split.is_none() => {
                    let selected = &document.headings()[outline.selected];
                    chrome.title = format!(
                        "tmd · 标题目录 · {}/{} · L{}",
                        outline.selected + 1,
                        document.headings().len(),
                        selected.source_line
                    );
                    chrome.details = outline.details(document.headings(), size);
                    state = format!(
                        "目录 · {}/{}",
                        outline.selected + 1,
                        document.headings().len()
                    );
                    overlay = outline.rows(document.headings(), size.0, theme, true);
                    let top = outline.top(overlay.len(), chrome.height(size.1));
                    (&overlay, top, None)
                }
                View::Outline => {
                    chrome.title = format!("正文 · {title}");
                    state = format!(
                        "目录 · {}/{}",
                        outline.selected + 1,
                        document.headings().len()
                    );
                    (
                        &lines,
                        start,
                        search
                            .current()
                            .and_then(|m| Document::match_line(&lines, m)),
                    )
                }
                View::Copy => {
                    chrome.title = format!(
                        "tmd · 代码块 · {}/{}",
                        picker.selected + 1,
                        document.code_blocks().len()
                    );
                    if !picker.notice.is_empty() && size.1 >= 4 {
                        chrome.details = reader_ui::wrapped(
                            picker.notice.trim_start_matches(" · "),
                            size.0.saturating_sub(1) as usize,
                            1,
                        );
                    }
                    state = format!(
                        "代码 · {}/{}",
                        picker.selected + 1,
                        document.code_blocks().len()
                    );
                    overlay = picker.rows(size.0);
                    let top = picker.top(chrome.height(size.1));
                    (&overlay, top, Some(picker.selected))
                }
                View::Help(pane) => {
                    chrome.title = "tmd · 帮助".into();
                    overlay = help_lines(size.0, pane, &fallback, &picker.notice);
                    help_start = help_start.min(last_start(&overlay, chrome.height(size.1)));
                    (&overlay, help_start, None)
                }
            };
            chrome.footer =
                reader_ui::footer(active_view, &state, !search.query.is_empty(), size.0);
            let (mut frame, end) = if let Some(width) = split {
                if active_view == View::Reading {
                    chrome.title = format!("正文 · {title}");
                }
                if active_view != View::Search {
                    chrome.footer = reader_ui::sidebar_footer(
                        active_view,
                        &state,
                        !search.query.is_empty(),
                        size.0,
                    );
                }
                let top = outline.top(document.headings().len(), size.1.saturating_sub(2) as usize);
                let sidebar = SidebarFrame {
                    width,
                    title: format!(
                        "标题目录 {}/{} · L{}",
                        outline.selected + 1,
                        document.headings().len(),
                        document.headings()[outline.selected].source_line,
                    ),
                    lines: outline.rows(
                        document.headings(),
                        width,
                        theme,
                        active_view == View::Outline,
                    ),
                    top,
                    focused: active_view == View::Outline,
                };
                paint_split(&lines, start, size, &chrome, highlight, &sidebar)
            } else {
                paint(display, position, size, &chrome, highlight)
            };
            frame.truncate(frame.len() - "\x1b[?2026l".len());
            let show_images =
                split.is_some() || matches!(active_view, View::Reading | View::Search);
            if show_images {
                images.request(&lines, start, end, size.1.saturating_sub(2) as usize);
            }
            images.paint(
                &mut frame,
                &lines,
                crate::reader_images::Viewport {
                    start,
                    end,
                    origin: split.map_or(1, |width| width + 2),
                    height: size.1.saturating_sub(2) as usize,
                    show: show_images,
                },
            );
            if view == View::Search {
                if let Some(query) = &search.input {
                    // The actual cursor remains inside the query for system IME.
                    if size.1 > 0 {
                        let (text, col) = query.visible(size.0.saturating_sub(2) as usize);
                        frame.push_str(&format!(
                            "\x1b[{};1H\x1b#5\x1b[0m\x1b[2K/{}\x1b[{};{}H\x1b[?25h",
                            size.1,
                            text,
                            size.1,
                            (col + 2).min(size.0.max(1) as usize)
                        ));
                    }
                }
            } else {
                frame.push_str("\x1b[?25l");
            }
            frame.push_str("\x1b[?2026l");
            checked_write(
                &mut io::stdout().lock(),
                frame.as_bytes(),
                &session.cancelled,
            )?;
            session.incomplete = false;
            if lines.is_empty() && queued.is_empty() && event::poll(Duration::ZERO)? {
                queued.push_back(event::read()?);
            }
            if lines.is_empty() && queued.is_empty() {
                session.exit_row = 1.min(size.1.saturating_sub(1));
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
                let (changed, layout_changed) = images.poll();
                if changed {
                    if layout_changed {
                        layouts.invalidate();
                    }
                    continue 'reader;
                }
                if show_images && images.needs_upload(&lines, start, end) {
                    continue 'reader;
                }
                if notice.expire(Instant::now()) {
                    continue 'reader;
                }
            };
            let action = match input {
                Event::Key(key) => key_action(key),
                _ => None,
            };
            if action == Some(Action::Interrupt) {
                session.cancelled.store(2, Ordering::SeqCst);
                break;
            }
            if let Event::Resize(cols, rows) = input {
                size = (cols, rows);

                continue;
            }
            if let Some(width) = split
                && matches!(active_view, View::Reading | View::Outline)
            {
                if action == Some(Action::Focus) {
                    view = if active_view == View::Outline {
                        View::Reading
                    } else {
                        View::Outline
                    };
                    continue;
                }
                if let Event::Mouse(mouse) = input
                    && matches!(
                        mouse.kind,
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                    )
                {
                    if mouse.row > 0 && mouse.row < size.1.saturating_sub(1) {
                        if mouse.column < width {
                            if let Some(amount) =
                                list_movement(None, &input, size.1.saturating_sub(2) as usize)
                            {
                                outline.move_by(amount, document.headings().len());
                            }
                        } else if mouse.column > width {
                            start = scroll_position(
                                start,
                                end,
                                last_start(&lines, chrome.height(size.1)),
                                chrome.height(size.1),
                                None,
                                &input,
                            );
                            anchor = Anchor::at(&lines, start);
                        }
                    }
                    continue;
                }
            }
            match active_view {
                View::Search => match input {
                    Event::Key(key) if key.kind != KeyEventKind::Release => {
                        match (key.code, action) {
                            (KeyCode::Esc, Some(Action::Exit)) => {
                                search.input = None;
                                anchor = search_anchor.clone();
                                // Resize already remaps saved_start. Without a
                                // resize, preserve even a decorative top row
                                // that shares its source range with the block.
                                start = search.saved_start;
                                view = View::Reading;
                            }
                            (KeyCode::Enter, Some(Action::Confirm)) => {
                                if search.confirm(&document)
                                    && let Some(row) = search
                                        .current()
                                        .and_then(|m| Document::match_line(&lines, m))
                                {
                                    anchor = Anchor::at(&lines, row);
                                    start = row
                                        .min(last_start(&lines, size.1.saturating_sub(2) as usize));
                                }
                                view = View::Reading;
                            }
                            _ => search
                                .input
                                .as_mut()
                                .expect("search view has input")
                                .key(key),
                        }
                    }
                    Event::Paste(text) => search
                        .input
                        .as_mut()
                        .expect("search view has input")
                        .insert(&text),
                    _ => {}
                },
                View::Outline | View::Copy => {
                    let pane = if active_view == View::Outline {
                        Pane::Outline
                    } else {
                        Pane::Copy
                    };
                    let close = action == Some(Action::Exit)
                        || (active_view == View::Outline && action == Some(Action::Outline))
                        || (active_view == View::Copy && action == Some(Action::Copy));
                    if close {
                        if active_view == View::Outline {
                            outline_open = false;
                        }
                        view = View::Reading;
                        picker.notice.clear();
                    } else if action == Some(Action::Help) {
                        view = View::Help(pane);
                        help_start = 0;
                    } else if action == Some(Action::Confirm) {
                        if active_view == View::Outline {
                            if let Some(row) = Document::heading_line(
                                &lines,
                                &document.headings()[outline.selected],
                            ) {
                                anchor = Anchor::at(&lines, row);
                                start =
                                    row.min(last_start(&lines, size.1.saturating_sub(2) as usize));
                                if split.is_none() {
                                    outline_open = false;
                                    view = View::Reading;
                                } else {
                                    view = View::Outline;
                                }
                                notice.show(
                                    format!(
                                        "已跳转 · L{}",
                                        document.headings()[outline.selected].source_line
                                    ),
                                    Instant::now(),
                                );
                            }
                        } else {
                            picker.confirm(document.code_blocks(), &mut copy);
                            if !picker.open {
                                view = View::Reading;
                                notice
                                    .show(picker.notice.trim_start_matches(" · "), Instant::now());
                                picker.notice.clear();
                            }
                        }
                    } else if let Some(amount) = list_movement(
                        action,
                        &input,
                        if split.is_some() {
                            size.1.saturating_sub(2) as usize
                        } else {
                            chrome.height(size.1)
                        },
                    ) {
                        if active_view == View::Outline {
                            outline.move_by(amount, document.headings().len());
                        } else {
                            picker.move_by(amount);
                        }
                    }
                }
                View::Help(pane) => {
                    if matches!(action, Some(Action::Exit | Action::Help)) {
                        view = pane.view();
                    } else {
                        help_start = scroll_position(
                            help_start,
                            end,
                            last_start(display, chrome.height(size.1)),
                            chrome.height(size.1),
                            action,
                            &input,
                        );
                    }
                }
                View::Reading => {
                    match action {
                        Some(Action::Exit) => {
                            if matches!(input, Event::Key(key) if key.code == KeyCode::Esc)
                                && !search.query.is_empty()
                            {
                                search.clear();
                            } else if matches!(input, Event::Key(key) if key.code == KeyCode::Esc)
                                && outline_open
                            {
                                outline_open = false;
                            } else {
                                break;
                            }
                        }
                        Some(Action::Search) => {
                            search.open(start);
                            search_anchor = Anchor::at(&lines, start);
                            view = View::Search;
                        }
                        Some(Action::Outline) if outline_open => {
                            outline_open = false;
                        }
                        Some(Action::Outline) => {
                            if outline.show(
                                document.headings(),
                                section_source(&document, &lines, start),
                            ) {
                                outline_open = true;
                                view = View::Outline;
                            } else {
                                notice.show("无可导航标题", Instant::now());
                                continue;
                            }
                        }
                        Some(Action::Copy) => {
                            picker.show(document.code_blocks(), &lines[start..]);
                            if picker.open {
                                view = View::Copy;
                            } else {
                                notice
                                    .show(picker.notice.trim_start_matches(" · "), Instant::now());
                                picker.notice.clear();
                                continue;
                            }
                        }
                        Some(Action::Help) => {
                            view = View::Help(Pane::Reading);
                            help_start = 0;
                        }
                        Some(Action::Next | Action::Previous) => {
                            search.advance(action == Some(Action::Previous));
                            if let Some(row) = search
                                .current()
                                .and_then(|m| Document::match_line(&lines, m))
                            {
                                anchor = Anchor::at(&lines, row);
                                start =
                                    row.min(last_start(&lines, size.1.saturating_sub(2) as usize));
                            }
                        }
                        _ => {
                            let next = scroll_position(
                                start,
                                end,
                                last_start(&lines, chrome.height(size.1)),
                                chrome.height(size.1),
                                action,
                                &input,
                            );
                            if start != next {
                                start = next;
                                anchor = Anchor::at(&lines, start);
                            }
                        }
                    }
                    if action.is_some()
                        || matches!(input, Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown))
                    {
                        notice.clear();
                    }
                }
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
    fn layout_cache_reuses_both_views_and_invalidates_on_resize() {
        let mut layouts = LayoutCache::new((80, 24));
        let mut lines = vec![reader_ui::line("full")];
        let full = lines.as_ptr();
        assert!(!layouts.update(&mut lines, (80, 24), None, || panic!("unchanged")));
        assert!(layouts.update(&mut lines, (80, 24), Some(24), || {
            vec![reader_ui::line("split")]
        }));
        let split = lines.as_ptr();
        for _ in 0..100 {
            assert!(layouts.update(&mut lines, (80, 24), None, || panic!("cached full")));
            assert_eq!(lines.as_ptr(), full);
            assert_eq!(lines[0].plain_text(), "full");
            assert!(layouts.update(&mut lines, (80, 24), Some(24), || panic!("cached split")));
            assert_eq!(lines.as_ptr(), split);
            assert_eq!(lines[0].plain_text(), "split");
        }
        assert!(layouts.update(&mut lines, (120, 24), Some(30), || {
            vec![reader_ui::line("wide split")]
        }));
        assert!(layouts.alternate.is_none());
        assert!(layouts.update(&mut lines, (120, 24), None, || {
            vec![reader_ui::line("wide full")]
        }));
        assert_eq!(lines[0].plain_text(), "wide full");
        assert!(layouts.update(&mut lines, (120, 1), None, || {
            vec![reader_ui::line("tiny height")]
        }));
        assert_eq!(lines[0].plain_text(), "tiny height");
        assert!(layouts.alternate.is_none());
    }

    #[test]
    fn sidebar_geometry_and_clipping_keep_panes_separate() {
        for (cols, expected) in [(79, None), (80, Some(24)), (120, Some(30)), (200, Some(36))] {
            assert_eq!(sidebar_width(cols, true), expected);
            assert_eq!(sidebar_width(cols, false), None);
        }
        let line = Line {
            spans: vec![
                Span::new("中é👩‍💻", Style::default()),
                Span::new("tail", Style::default()),
            ],
            ..row(LineSize::Normal)
        };
        let mut out = String::new();
        pane_line(&mut out, &line, 4, false);
        assert_eq!(out, "中é"); // The joined emoji and every following span are clipped together.
        out.clear();
        pane_line(&mut out, &line, 5, false);
        assert_eq!(out, "中é👩‍💻");
        let sidebar = SidebarFrame {
            width: 24,
            title: "目录".into(),
            lines: vec![reader_ui::line("heading")],
            top: 0,
            focused: true,
        };
        let lines = vec![reader_ui::line("x".repeat(100))];
        let (frame, end) = paint_split(&lines, 0, (80, 4), &Chrome::default(), None, &sidebar);
        assert_eq!(end, 1);
        assert!(frame.contains(&format!("\x1b[2;26H{}", "x".repeat(54))));
        assert!(!frame.contains(&"x".repeat(55)));
        assert!(!frame.contains("\x1b#3") && !frame.contains("\x1b#4"));
        assert!(!frame.contains("?1049") && !frame.contains("\x1b[3J"));
    }

    #[test]
    fn modified_keys_cannot_trigger_reader_commands() {
        for code in [KeyCode::Char('q'), KeyCode::Char('?'), KeyCode::Down] {
            for modifiers in [
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::SUPER,
            ] {
                assert_eq!(key_action(KeyEvent::new(code, modifiers)), None);
            }
        }
        assert_eq!(
            key_action(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Interrupt)
        );
        assert_eq!(
            key_action(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)),
            Some(Action::Help)
        );
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
        let (frame, end) = paint(
            &lines,
            0,
            (40, 4),
            &Chrome {
                title: "tmd test.md".into(),
                ..Chrome::default()
            },
            None,
        );
        assert_eq!(end, 1);
        assert!(!frame.contains("\x1b#3"));
        let (frame, end) = paint(
            &lines,
            1,
            (40, 4),
            &Chrome {
                title: "tmd test.md".into(),
                ..Chrome::default()
            },
            None,
        );
        assert_eq!(end, 2);
        assert_eq!(frame.matches("\x1b#3").count(), 1);
        assert_eq!(frame.matches("\x1b#4").count(), 1);
        assert!(!frame.contains("?1049"));
        assert!(!frame.contains("\x1b[3J"));
    }
    #[test]
    fn resize_can_show_a_tiny_window_without_leaking_body_rows() {
        let (frame, end) = paint(
            &[row(LineSize::Double)],
            0,
            (4, 1),
            &Chrome {
                title: "header".into(),
                ..Chrome::default()
            },
            None,
        );
        assert_eq!(end, 0);
        assert!(!frame.contains("\x1b#3"));
        assert!(!frame.contains("header"));
    }

    #[test]
    fn empty_heading_is_a_section_even_without_searchable_text() {
        let doc = Document::new("intro\n\n###\n\n## Next\n\nbody", Theme::default());
        let lines = doc.layout(80, RenderOptions::default());
        let heading = &doc.headings()[0];
        let row = Document::heading_line(&lines, heading).unwrap();
        assert_eq!(section_source(&doc, &lines, row), heading.source.start);
    }
}
