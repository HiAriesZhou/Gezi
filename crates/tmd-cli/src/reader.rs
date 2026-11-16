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
use crate::reader_copy::{self, CopyButtons};
use crate::reader_outline::Outline;
use crate::reader_search::Search;
use crate::reader_selection::{self, Selection};
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
    reader_image_layout(cli, document, size, open, None, true)
}

const DEFAULT_READING_WIDTH: u16 = 100;

fn reader_image_layout(
    cli: &Cli,
    document: &Document,
    size: (u16, u16),
    open: bool,
    images: Option<&tmd_core::ImageLayout<'_>>,
    copy_buttons: bool,
) -> Vec<Line> {
    let sidebar = sidebar_width(size.0, open);
    let available = size.0.saturating_sub(sidebar.map_or(0, |w| w + 1));
    let gutter = 2.min(available.saturating_sub(2) / 2);
    let width = cli
        .width
        .unwrap_or(DEFAULT_READING_WIDTH)
        .min(available.saturating_sub(2 * gutter))
        .max(1);
    let large =
        sidebar.is_none() && size.1 >= 4 && large_headings::enabled(cli, true, Some(size), width);
    let options = RenderOptions {
        large_headings: large,
        code_copy_buttons: copy_buttons,
    };
    let lines = match images {
        Some(images) => document.layout_with_images(width, options, images),
        None => document.layout(width, options),
    };
    let inset = available.saturating_sub(width) / 2;
    inset_lines(lines, if large { inset / 2 * 2 } else { inset })
}

fn inset_lines(lines: Vec<Line>, inset: u16) -> Vec<Line> {
    lines
        .into_iter()
        .map(|line| {
            if inset == 0 || line.spans.is_empty() {
                return line;
            }
            let columns = if line.size == LineSize::Double {
                inset / 2
            } else {
                inset
            };
            Line {
                spans: std::iter::once(tmd_core::Span::new(
                    " ".repeat(columns as usize),
                    tmd_core::Style::default(),
                ))
                .chain(line.spans)
                .collect(),
                ..line
            }
        })
        .collect()
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
                std::ffi::OsString::from("gezi")
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
struct Chrome<'a> {
    search: Option<&'a Search>,
    copy_buttons: Option<&'a CopyButtons>,
    selection: Option<&'a Selection>,
    title: String,
    details: Vec<Line>,
    footer: String,
}

impl Chrome<'_> {
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
            let marked = chrome.search.map(|search| search.highlight(line));
            let line = marked.as_ref().unwrap_or(line);
            let copied = chrome
                .copy_buttons
                .and_then(|c| c.decorate(line, Instant::now()));
            let line = copied.as_ref().unwrap_or(line);
            let selected = chrome
                .selection
                .and_then(|selection| selection.highlight(line, start + index));
            let line = selected.as_ref().unwrap_or(line);
            let mut text = String::new();
            if highlight == Some(start + index) {
                let mut marked = line.clone();
                let mapped = marked.spans.iter().any(|span| !span.text_ranges.is_empty());
                for span in &mut marked.spans {
                    if !mapped || !span.text_ranges.is_empty() {
                        span.style.bold = true;
                        span.style.underline = true;
                    }
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
    let mapped = clipped
        .spans
        .iter()
        .any(|span| !span.text_ranges.is_empty());
    let mut remaining = width;
    for span in &mut clipped.spans {
        let part = truncate(&span.text, remaining).to_owned();
        remaining = remaining.saturating_sub(tmd_core::text_width(&part));
        if part.len() != span.text.len() {
            remaining = 0;
        }
        span.text = part;
        if selected && (!mapped || !span.text_ranges.is_empty()) {
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
            let marked = chrome.search.map(|search| search.highlight(line));
            let line = marked.as_ref().unwrap_or(line);
            let copied = chrome
                .copy_buttons
                .and_then(|c| c.decorate(line, Instant::now()));
            let line = copied.as_ref().unwrap_or(line);
            let selected = chrome
                .selection
                .and_then(|selection| selection.highlight(line, index));
            pane_line(
                &mut out,
                selected.as_ref().unwrap_or(line),
                body_width,
                highlight == Some(index),
            );
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
    Outline,
    Confirm,
    Focus,
    Language,
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
        KeyCode::Char('o') => Action::Outline,
        KeyCode::Char('L') | KeyCode::F(2) if key.kind == KeyEventKind::Press => Action::Language,
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

fn native_fallback() -> &'static str {
    crate::i18n::text(
        "Native positioning unavailable; using inline reader",
        "原生定位不可用，使用内置阅读",
    )
}

fn help_lines(width: u16, pane: Pane, fallback: &str) -> Vec<Line> {
    let common = [
        crate::i18n::text(
            "L / F2: switch English/Chinese (F2 while typing a search); this session only",
            "L / F2：切换中英文（搜索输入时用 F2）；仅影响当前会话",
        ),
        crate::i18n::text(
            "↑↓ / j k / wheel: scroll; PageUp/b, PageDown/Space: page; Home/End: start/end",
            "↑↓ / j k / 滚轮：移动；PageUp/b、PageDown/空格：翻页；Home/End：首尾",
        ),
        crate::i18n::text(
            "? / Esc / q: close help and return to the previous view",
            "? / Esc / q：关闭帮助，返回打开帮助前的界面",
        ),
        crate::i18n::text(
            "Ctrl-C: interrupt, restore the terminal and return to the shell",
            "Ctrl-C：中断，恢复终端并返回命令行",
        ),
        crate::i18n::text(
            "Click Copy at the top right of a code block to copy its original text in place",
            "点击代码块右上角“复制”复制完整代码；不离开正文",
        ),
        crate::i18n::text(
            "Drag document text and release to copy; a click alone does not copy",
            "正文中直接拖选文字，松开左键复制；单击不复制",
        ),
    ];
    let specific: &[&str] = match pane {
        Pane::Reading => &[
            crate::i18n::text("Reading help", "阅读帮助"),
            crate::i18n::text(
                "/: search document text (case-sensitive, Enter confirms); n/N: next/previous match",
                "/：搜索正文（区分大小写，Enter 确认）；n/N：下一处/上一处匹配",
            ),
            crate::i18n::text(
                "Search input: Esc cancels, Ctrl-U clears; letters including L remain query text",
                "搜索输入中 Esc 取消；Ctrl-U 清空；q、o、c、L 等作为查询文字",
            ),
            crate::i18n::text(
                "o: toggle outline; Tab: switch sidebar/document focus; ?: help",
                "o：开关标题目录；Tab：侧栏与正文切换焦点；?：帮助",
            ),
            crate::i18n::text(
                "q quits reading; Esc clears search, then closes the sidebar, then quits",
                "阅读时 q 直接退出；Esc 依次清除搜索、关闭侧栏、退出",
            ),
        ],
        Pane::Outline => &[
            crate::i18n::text("Outline help", "目录帮助"),
            crate::i18n::text(
                "Background: selected heading; ▎: current section; L: source line number",
                "背景为选中项；▎ 为当前阅读章节；L 为源码行号",
            ),
            crate::i18n::text(
                "Enter: jump while keeping outline focus; Tab: focus the document",
                "Enter：跳转并保留目录焦点，可连续选择跳转；Tab：切换到正文",
            ),
            crate::i18n::text(
                "In narrow windows, Enter jumps and closes the outline",
                "窄窗口中 Enter 跳转后关闭目录",
            ),
            crate::i18n::text(
                "Esc / q / o: close outline; wheel scrolls the pane under the pointer",
                "Esc / q / o：关闭目录；滚轮只移动指针所在区域",
            ),
            crate::i18n::text(
                "At 80+ columns, use a sidebar and normal headings; narrower windows use a full-page outline",
                "80 列起显示侧栏并使用普通标题，窄窗口使用整页目录",
            ),
            crate::i18n::text(
                "Selecting does not move the document. Duplicate and untitled headings have separate destinations",
                "选择不会移动正文。重复和空标题按各自源码位置跳转",
            ),
            crate::i18n::text(
                "Outline jumps retain the search; n/N resumes from the selected match",
                "目录跳转保留搜索；n/N 仍从原先选中的匹配继续",
            ),
        ],
    };
    specific
        .iter()
        .chain(common.iter())
        .copied()
        .chain(std::iter::once(fallback).filter(|s| !s.is_empty()))
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
    let mut lines = reader_image_layout(
        cli,
        &document,
        size,
        false,
        None,
        mode != ReaderMode::Native,
    );
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
        notice = crate::i18n::text(
            " · Native positioning unavailable; using inline reader",
            " · 原生定位不可用，使用内置阅读",
        )
        .into();
        lines = layout(cli, &document, size);
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
        let mut copy_buttons = CopyButtons::default();
        let mut selection = Selection::default();
        let mut outline = Outline::default();
        let fallback = !notice.is_empty();
        let mut notice = Notice::default();
        if fallback {
            notice.show(native_fallback(), Instant::now());
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
                reader_image_layout(
                    cli,
                    &document,
                    size,
                    outline_open,
                    images.layout().as_ref(),
                    true,
                )
            }) {
                selection.clear();
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
                            &transient.map_or_else(
                                || crate::i18n::trf!("Search{}", "搜索{}", search.notice()),
                                str::to_owned,
                            ),
                            false,
                            split.map_or(size.0, |w| size.0 - w - 1),
                        )
                    } else {
                        title.clone()
                    };
                    let end = visible_end(&lines, start, chrome.height(size.1));
                    let position = if start == 0 {
                        crate::i18n::text("Start", "开头")
                    } else if end == lines.len() {
                        crate::i18n::text("End", "末尾")
                    } else {
                        crate::i18n::text("In document", "阅读中")
                    };
                    state = if search.query.is_empty() {
                        crate::i18n::trf!("Reading · {position}", "阅读 · {position}")
                    } else {
                        search.notice().trim_start_matches(" · ").into()
                    };
                    if let Some(message) = transient {
                        state = message.into();
                    }
                    (&lines, start, None)
                }
                View::Outline if split.is_none() => {
                    let selected = &document.headings()[outline.selected];
                    chrome.title = crate::i18n::trf!(
                        "Gezi · Outline · {}/{} · L{}",
                        "Gezi · 标题目录 · {}/{} · L{}",
                        outline.selected + 1,
                        document.headings().len(),
                        selected.source_line
                    );
                    chrome.details = outline.details(document.headings(), size);
                    state = crate::i18n::trf!(
                        "Outline · {}/{}",
                        "目录 · {}/{}",
                        outline.selected + 1,
                        document.headings().len()
                    );
                    overlay = outline.rows(document.headings(), size.0, theme, true);
                    let top = outline.top(overlay.len(), chrome.height(size.1));
                    (&overlay, top, None)
                }
                View::Outline => {
                    chrome.title = crate::i18n::trf!("Document · {title}", "正文 · {title}");
                    state = crate::i18n::trf!(
                        "Outline · {}/{}",
                        "目录 · {}/{}",
                        outline.selected + 1,
                        document.headings().len()
                    );
                    (&lines, start, None)
                }
                View::Help(pane) => {
                    chrome.title = crate::i18n::text("Gezi · Help", "Gezi · 帮助").into();
                    overlay =
                        help_lines(size.0, pane, if fallback { native_fallback() } else { "" });
                    help_start = help_start.min(last_start(&overlay, chrome.height(size.1)));
                    (&overlay, help_start, None)
                }
            };
            if let Some(message) = transient {
                state = message.into();
            }
            chrome.footer =
                reader_ui::footer(active_view, &state, !search.query.is_empty(), size.0);
            if matches!(active_view, View::Reading | View::Search)
                || (active_view == View::Outline && split.is_some())
            {
                chrome.search = Some(&search);
                chrome.copy_buttons = Some(&copy_buttons);
                chrome.selection = Some(&selection);
            }
            let (mut frame, end) = if let Some(width) = split {
                if active_view == View::Reading {
                    chrome.title = crate::i18n::trf!("Document · {title}", "正文 · {title}");
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
                    title: crate::i18n::trf!(
                        "Outline {}/{} · L{}",
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
            let body_height = chrome.height(size.1);
            drop(chrome);
            let buttons = if matches!(active_view, View::Reading | View::Search) || split.is_some()
            {
                reader_copy::buttons(&lines, start, end, split.map_or(0, |w| w + 1), size.0)
            } else {
                Vec::new()
            };
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
                let now = Instant::now();
                let feedback_expired = copy_buttons.expire(now);
                if notice.expire(now) || feedback_expired {
                    continue 'reader;
                }
            };
            if let Some(message) = copy_buttons.event(
                &input,
                &buttons,
                document.code_blocks(),
                &mut copy,
                Instant::now(),
            ) {
                selection.clear();
                notice.show(message, Instant::now());
                continue;
            }
            let document_view = matches!(active_view, View::Reading | View::Search)
                || (active_view == View::Outline && split.is_some());
            if document_view {
                let viewport = reader_selection::Viewport {
                    lines: &lines,
                    start,
                    end,
                    origin: split.map_or(0, |width| width + 1),
                    size,
                };
                let (handled, result) = selection.event(&input, &viewport, &buttons, &mut copy);
                if let Some(result) = result {
                    notice.show(
                        match result {
                            Ok(()) => {
                                crate::i18n::text("Selected text copied", "已复制所选文字").into()
                            }
                            Err(error) => error.to_string(),
                        },
                        Instant::now(),
                    );
                }
                if handled {
                    if active_view == View::Outline && selection.dragging {
                        view = View::Reading;
                    }
                    continue;
                }
            } else {
                selection.clear();
            }
            let action = match input {
                Event::Key(key) => key_action(key),
                _ => None,
            };
            if action == Some(Action::Language)
                && (active_view != View::Search
                    || matches!(input, Event::Key(key) if key.code == KeyCode::F(2)))
            {
                crate::i18n::toggle();
                selection.clear();
                notice.show(
                    crate::i18n::text("Language: English", "语言：中文"),
                    Instant::now(),
                );
                continue;
            }
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
                                last_start(&lines, body_height),
                                body_height,
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
                View::Search => {
                    match input {
                        Event::Key(key) if key.kind != KeyEventKind::Release => {
                            match (key.code, action) {
                                (KeyCode::Esc, Some(Action::Exit)) => {
                                    search.cancel();
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
                                        start = row.min(last_start(
                                            &lines,
                                            size.1.saturating_sub(2) as usize,
                                        ));
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
                    }
                    if view == View::Search && search.preview(&document) {
                        if let Some(row) = search
                            .current()
                            .and_then(|m| Document::match_line(&lines, m))
                        {
                            start = row.min(last_start(&lines, size.1.saturating_sub(2) as usize));
                            anchor = Anchor::at(&lines, start);
                        } else {
                            start = search.saved_start;
                            anchor = search_anchor.clone();
                        }
                    }
                }
                View::Outline => {
                    let close = action == Some(Action::Exit) || action == Some(Action::Outline);
                    if close {
                        outline_open = false;
                        view = View::Reading;
                    } else if action == Some(Action::Help) {
                        view = View::Help(Pane::Outline);
                        help_start = 0;
                    } else if action == Some(Action::Confirm) {
                        if let Some(row) =
                            Document::heading_line(&lines, &document.headings()[outline.selected])
                        {
                            anchor = Anchor::at(&lines, row);
                            start = row.min(last_start(&lines, size.1.saturating_sub(2) as usize));
                            if split.is_none() {
                                outline_open = false;
                                view = View::Reading;
                            }
                            notice.show(
                                crate::i18n::trf!(
                                    "Jumped · L{}",
                                    "已跳转 · L{}",
                                    document.headings()[outline.selected].source_line
                                ),
                                Instant::now(),
                            );
                        }
                    } else if let Some(amount) = list_movement(
                        action,
                        &input,
                        if split.is_some() {
                            size.1.saturating_sub(2) as usize
                        } else {
                            body_height
                        },
                    ) {
                        outline.move_by(amount, document.headings().len());
                    }
                }
                View::Help(pane) => {
                    if matches!(action, Some(Action::Exit | Action::Help)) {
                        view = pane.view();
                    } else {
                        help_start = scroll_position(
                            help_start,
                            end,
                            last_start(display, body_height),
                            body_height,
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
                                notice.show(
                                    crate::i18n::text("No headings to navigate", "无可导航标题"),
                                    Instant::now(),
                                );
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
                                last_start(&lines, body_height),
                                body_height,
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
    fn reading_column_is_centered_inside_the_document_pane() {
        let _language = crate::i18n::scoped(crate::i18n::Language::Chinese);
        use clap::Parser;
        for (cols, open, width, expected_inset) in [
            (160, false, Some(100), 30),
            (160, false, None, 30),
            (200, false, None, 50),
            (200, true, None, 31),
            (160, false, Some(120), 20),
            (100, false, Some(40), 30),
            (100, true, Some(40), 17),
            (80, false, None, 2),
            (40, false, Some(100), 2),
            (4, false, None, 1),
            (2, false, None, 0),
        ] {
            let mut args = vec!["gezi".to_owned()];
            if let Some(width) = width {
                args.extend(["--width".into(), width.to_string()]);
            }
            let cli = Cli::parse_from(args);
            let doc = Document::new("中\n\n## Next", Theme::default());
            let lines = reader_layout(&cli, &doc, (cols, 24), open);
            let first = &lines[0];
            assert_eq!(
                first.plain_text().len() - first.plain_text().trim_start().len(),
                expected_inset
            );
            let matched = Document::match_line(&lines, &doc.find("中")[0]).unwrap();
            assert_eq!(matched, 0);
            if expected_inset > 0 {
                assert!(first.spans[0].text_ranges.is_empty());
            }
            let heading = Document::heading_line(&lines, &doc.headings()[0]).unwrap();
            assert_eq!(heading, 3);
            assert!(lines[1].spans.is_empty() && lines[2].spans.is_empty());
            let available = cols - sidebar_width(cols, open).map_or(0, |w| w + 1);
            assert!(lines.iter().all(|line| line.width() <= available as usize));
        }
    }

    #[test]
    fn reader_insets_preserve_line_sizes_and_keep_highlights_inside_content() {
        let _language = crate::i18n::scoped(crate::i18n::Language::Chinese);
        let empty = Line {
            spans: Vec::new(),
            ..row(LineSize::Normal)
        };
        let lines = inset_lines(vec![row(LineSize::Normal), row(LineSize::Double), empty], 4);
        assert_eq!(lines[0].spans[0].width(), 4);
        assert_eq!(lines[1].spans[0].width(), 2);
        assert_eq!(lines[1].size, LineSize::Double);
        assert!(lines[2].spans.is_empty());
        assert!(lines.iter().all(|line| line.src == (0..20)));
        use clap::Parser;
        let cli = Cli::parse_from(["gezi", "--width", "40"]);
        let doc = Document::new("needle", Theme::default());
        let lines = reader_layout(&cli, &doc, (100, 24), false);
        let (frame, _) = paint(&lines, 0, (100, 24), &Chrome::default(), Some(0));
        assert!(frame.contains(&format!("\x1b[2;1H{}\x1b[1;4mneedle", " ".repeat(30))));
        let mut split = String::new();
        pane_line(&mut split, &lines[0], 100, true);
        assert!(split.starts_with(&format!("{}\x1b[1;4mneedle", " ".repeat(30))));
    }

    #[test]
    fn layout_cache_reuses_both_views_and_invalidates_on_resize() {
        let _language = crate::i18n::scoped(crate::i18n::Language::Chinese);
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
        let _language = crate::i18n::scoped(crate::i18n::Language::Chinese);
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
        let _language = crate::i18n::scoped(crate::i18n::Language::Chinese);
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
        let _language = crate::i18n::scoped(crate::i18n::Language::Chinese);
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
                title: "gezi test.md".into(),
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
                title: "gezi test.md".into(),
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
        let _language = crate::i18n::scoped(crate::i18n::Language::Chinese);
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
        let _language = crate::i18n::scoped(crate::i18n::Language::Chinese);
        let doc = Document::new("intro\n\n###\n\n## Next\n\nbody", Theme::default());
        let lines = doc.layout(80, RenderOptions::default());
        let heading = &doc.headings()[0];
        let row = Document::heading_line(&lines, heading).unwrap();
        assert_eq!(section_source(&doc, &lines, row), heading.source.start);
    }
}
