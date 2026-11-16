//! Reader-owned mouse selection. Coordinates refer to laid-out document rows.
use crate::clipboard::CopyError;
use crate::reader_copy::Button;
use crossterm::event::{Event, MouseButton, MouseEventKind};
use std::ops::Range;
use tmd_core::{Color, Line, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Point {
    pub row: usize,
    pub column: usize,
}

pub(crate) struct Viewport<'a> {
    pub lines: &'a [Line],
    pub start: usize,
    pub end: usize,
    pub origin: u16,
    pub size: (u16, u16),
}

#[derive(Default)]
pub(crate) struct Selection {
    endpoints: Option<(Point, Point)>,
    pub dragging: bool,
    moved: bool,
}

impl Selection {
    /// Copy controls own gestures started on them, even if the drag leaves the button.
    pub fn event(
        &mut self,
        event: &Event,
        viewport: &Viewport<'_>,
        buttons: &[Button],
        copy: &mut impl FnMut(&str) -> Result<(), CopyError>,
    ) -> (bool, Option<Result<(), CopyError>>) {
        let Event::Mouse(mouse) = event else {
            self.clear();
            return (false, None);
        };
        if !mouse.modifiers.is_empty() {
            self.clear();
            return (false, None);
        }
        let point = point(
            viewport.lines,
            viewport.start,
            viewport.end,
            viewport.origin,
            viewport.size,
            (mouse.column, mouse.row),
            self.dragging,
        );
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.clear();
                let on_button = buttons.iter().any(|b| {
                    b.row == mouse.row && b.left <= mouse.column && mouse.column < b.right
                });
                if !on_button && let Some(point) = point {
                    self.begin(point);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(point) = point {
                    self.extend(point);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(point) = point {
                    self.extend(point);
                }
                return (true, self.finish(viewport.lines, copy));
            }
            MouseEventKind::Moved => {}
            _ => {
                self.clear();
                return (false, None);
            }
        }
        (true, None)
    }

    pub fn begin(&mut self, point: Point) {
        self.endpoints = Some((point, point));
        self.dragging = true;
        self.moved = false;
    }
    pub fn extend(&mut self, point: Point) {
        if self.dragging
            && let Some((start, end)) = &mut self.endpoints
        {
            self.moved |= point != *start;
            *end = point;
        }
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    fn columns(&self, row: usize) -> Option<Range<usize>> {
        if !self.moved {
            return None;
        }
        let (a, b) = self.endpoints?;
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        (start.row <= row && row <= end.row).then(|| {
            let left = if row == start.row { start.column } else { 0 };
            let right = if row == end.row {
                end.column.saturating_add(1)
            } else {
                usize::MAX
            };
            left..right
        })
    }
    pub fn highlight(&self, line: &Line, row: usize) -> Option<Line> {
        self.columns(row)
            .map(|columns| line.select_columns(columns, &Self::style()).0)
    }
    fn style() -> Style {
        Style::fg(Color::Ansi(15)).on(Color::Ansi(4))
    }
    pub fn finish(
        &mut self,
        lines: &[Line],
        copy: &mut impl FnMut(&str) -> Result<(), CopyError>,
    ) -> Option<Result<(), CopyError>> {
        if !self.dragging {
            return None;
        }
        self.dragging = false;
        let (a, b) = self.endpoints?;
        if !self.moved {
            return None;
        }
        let mut text = String::new();
        for row in a.row.min(b.row)..=a.row.max(b.row) {
            let line = lines.get(row)?;
            if row > a.row.min(b.row) {
                text.push('\n');
            }
            let (_, part) = line.select_columns(self.columns(row)?, &Self::style());
            text.push_str(part.trim_end_matches(' '));
        }
        if text.trim().is_empty() {
            return None;
        }
        Some(copy(&text))
    }
}

/// Map screen cells to document coordinates, including both halves of large H1.
/// Ignore the sidebar/header/footer for a new selection; clamp an active drag.
pub(crate) fn point(
    lines: &[Line],
    start: usize,
    end: usize,
    origin: u16,
    size: (u16, u16),
    cursor: (u16, u16),
    clamp: bool,
) -> Option<Point> {
    let (x, y) = cursor;
    if size.1 < 3 || start >= end {
        return None;
    }
    if !clamp && (x < origin || x >= size.0.saturating_sub(1) || y == 0 || y >= size.1 - 1) {
        return None;
    }
    let y = y.clamp(1, size.1 - 2) as usize;
    let mut screen_row = 1;
    for (row, line) in lines.iter().enumerate().take(end).skip(start) {
        if y < screen_row + line.physical_height() || (clamp && row + 1 == end) {
            return Some(Point {
                row,
                column: (x.saturating_sub(origin) as usize / line.physical_height())
                    .min(line.width() / line.physical_height()),
            });
        }
        screen_row += line.physical_height();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmd_core::{LineSize, Span};
    fn line(text: &str) -> Line {
        Line {
            spans: vec![Span::new(text, Style::default())],
            src: 0..0,
            size: LineSize::Normal,
        }
    }
    fn mouse(kind: MouseEventKind, x: u16, y: u16) -> Event {
        Event::Mouse(crossterm::event::MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        })
    }

    #[test]
    fn text_drags_and_copy_buttons_never_share_a_gesture() {
        use crate::reader_copy::{self, CopyButtons};
        use tmd_core::{Document, RenderOptions, Theme};
        let doc = Document::new("```\none two\nthree\n```", Theme::default());
        let lines = doc.layout(
            30,
            RenderOptions {
                code_copy_buttons: true,
                ..Default::default()
            },
        );
        let viewport = Viewport {
            lines: &lines,
            start: 0,
            end: lines.len(),
            origin: 0,
            size: (80, 24),
        };
        let buttons = reader_copy::buttons(&lines, 0, lines.len(), 0, 80);
        let b = buttons[0];
        let down = MouseEventKind::Down(MouseButton::Left);
        let drag = MouseEventKind::Drag(MouseButton::Left);
        let up = MouseEventKind::Up(MouseButton::Left);
        for (events, expected) in [
            (
                vec![
                    mouse(down, 2, 2),
                    mouse(drag, 4, 2),
                    mouse(up, 4, 2),
                    mouse(up, 4, 2),
                ],
                vec!["one"],
            ),
            (
                vec![mouse(down, 8, 2), mouse(drag, 6, 2), mouse(up, 6, 2)],
                vec!["two"],
            ),
            (
                vec![mouse(down, b.left, b.row), mouse(up, b.left, b.row)],
                vec!["one two\nthree\n"],
            ),
            (
                vec![
                    mouse(down, b.left, b.row),
                    mouse(drag, 4, 2),
                    mouse(up, 4, 2),
                ],
                vec![],
            ),
            (
                vec![
                    mouse(down, b.left, b.row),
                    mouse(drag, 4, 2),
                    mouse(up, b.left, b.row),
                ],
                vec![],
            ),
        ] {
            let mut copies = Vec::new();
            let mut copy = |s: &str| {
                copies.push(s.to_owned());
                Ok(())
            };
            let mut controls = CopyButtons::default();
            let mut selection = Selection::default();
            for e in events {
                if controls
                    .event(
                        &e,
                        &buttons,
                        doc.code_blocks(),
                        &mut copy,
                        std::time::Instant::now(),
                    )
                    .is_some()
                {
                    selection.clear();
                } else {
                    selection.event(&e, &viewport, &buttons, &mut copy);
                }
            }
            assert_eq!(copies, expected);
        }
        // A text drag ending on Copy still copies only selected display text.
        let mut s = Selection::default();
        let mut copies = Vec::new();
        let mut copy = |s: &str| {
            copies.push(s.to_owned());
            Ok(())
        };
        for e in [
            mouse(down, 2, 2),
            mouse(drag, b.left, b.row),
            mouse(up, b.left, b.row),
        ] {
            s.event(&e, &viewport, &buttons, &mut copy);
        }
        // Displayed indentation is part of a multiline drag selection.
        assert_eq!(copies, ["\n  o"]);
        let marked = s.highlight(&lines[0], 0).unwrap();
        let control = marked.spans.iter().find(|s| s.code_copy.is_some()).unwrap();
        assert_ne!(control.style.bg, Selection::style().bg);
    }

    #[test]
    fn keyboard_paste_resize_wheel_and_modified_release_cancel_drag() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let lines = vec![line("hello")];
        let v = Viewport {
            lines: &lines,
            start: 0,
            end: 1,
            origin: 0,
            size: (80, 24),
        };
        let mut modified = mouse(MouseEventKind::Up(MouseButton::Left), 3, 1);
        if let Event::Mouse(ref mut m) = modified {
            m.modifiers = KeyModifiers::SHIFT;
        }
        for cancel in [
            Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Event::Paste("query".into()),
            Event::Resize(10, 10),
            Event::FocusLost,
            mouse(MouseEventKind::ScrollDown, 3, 1),
            modified,
        ] {
            let mut s = Selection::default();
            let mut forbidden =
                |_: &str| -> Result<(), CopyError> { panic!("cancelled drag copied") };
            s.event(
                &mouse(MouseEventKind::Down(MouseButton::Left), 0, 1),
                &v,
                &[],
                &mut forbidden,
            );
            s.event(
                &mouse(MouseEventKind::Drag(MouseButton::Left), 3, 1),
                &v,
                &[],
                &mut forbidden,
            );
            assert!(s.highlight(&lines[0], 0).is_some());
            s.event(&cancel, &v, &[], &mut forbidden);
            s.event(
                &mouse(MouseEventKind::Up(MouseButton::Left), 3, 1),
                &v,
                &[],
                &mut forbidden,
            );
            assert!(s.highlight(&lines[0], 0).is_none());
        }
    }

    #[test]
    fn direct_drag_copies_once_in_either_direction_and_preserves_clusters() {
        let lines = vec![line("中文 é 👩‍💻"), line("second")];
        for reverse in [false, true] {
            let mut s = Selection::default();
            let a = Point { row: 0, column: 1 };
            let b = Point { row: 1, column: 2 };
            s.begin(if reverse { b } else { a });
            s.extend(if reverse { a } else { b });
            let mut copies = Vec::new();
            let mut copy = |text: &str| {
                copies.push(text.to_owned());
                Ok(())
            };
            assert_eq!(s.finish(&lines, &mut copy), Some(Ok(())));
            assert_eq!(s.finish(&lines, &mut copy), None);
            assert_eq!(copies, ["中文 é 👩‍💻\nsec"]);
            let marked = s.highlight(&lines[0], 0).unwrap();
            assert_eq!(marked.plain_text(), lines[0].plain_text());
            assert_eq!(marked.width(), lines[0].width());
        }
    }
    #[test]
    fn clicks_cancel_and_blank_drags_never_write_clipboard() {
        let lines = vec![line("   ")];
        let mut s = Selection::default();
        let mut forbidden =
            |_: &str| -> Result<(), CopyError> { panic!("unexpected clipboard write") };
        s.begin(Point { row: 0, column: 0 });
        assert_eq!(s.finish(&lines, &mut forbidden), None);
        s.begin(Point { row: 0, column: 0 });
        s.extend(Point { row: 0, column: 2 });
        assert_eq!(s.finish(&lines, &mut forbidden), None);
        s.clear();
        assert_eq!(s.finish(&lines, &mut forbidden), None);
    }
    #[test]
    fn clipboard_failure_is_returned_without_losing_highlight() {
        let lines = vec![line("hello")];
        let mut s = Selection::default();
        s.begin(Point { row: 0, column: 0 });
        s.extend(Point { row: 0, column: 2 });
        assert_eq!(
            s.finish(&lines, &mut |_| Err(CopyError::Unsupported)),
            Some(Err(CopyError::Unsupported))
        );
        assert!(s.highlight(&lines[0], 0).is_some());
    }
    #[test]
    fn blank_area_cannot_start_a_selection_but_can_extend_one() {
        let lines = vec![line("hello")];
        assert_eq!(point(&lines, 0, 1, 0, (80, 24), (2, 10), false), None);
        assert_eq!(
            point(&lines, 0, 1, 0, (80, 24), (2, 10), true),
            Some(Point { row: 0, column: 2 })
        );
    }

    #[test]
    fn screen_mapping_handles_inset_sidebar_large_headings_and_edges() {
        let mut lines = vec![line("  Heading"), line("  body")];
        lines[0].size = LineSize::Double;
        assert_eq!(
            point(&lines, 0, 2, 0, (80, 24), (6, 2), false),
            Some(Point { row: 0, column: 3 })
        );
        assert_eq!(point(&lines, 0, 2, 25, (80, 24), (24, 2), false), None);
        assert_eq!(
            point(&lines, 0, 2, 25, (80, 24), (29, 3), false),
            Some(Point { row: 1, column: 4 })
        );
        assert_eq!(point(&lines, 0, 2, 0, (80, 24), (2, 0), false), None);
        assert_eq!(
            point(&lines, 0, 2, 0, (80, 24), (200, 200), true),
            Some(Point { row: 1, column: 6 })
        );
    }
}
