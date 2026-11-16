//! Inline code copy controls. Tests inject a backend and never use the clipboard.
use crate::clipboard::CopyError;
use crossterm::event::{Event, MouseButton, MouseEventKind};
use std::time::{Duration, Instant};
use tmd_core::{CodeSnippet, Line};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Button {
    pub code: usize,
    pub row: u16,
    pub left: u16,
    pub right: u16,
}

pub(crate) fn buttons(
    lines: &[Line],
    start: usize,
    end: usize,
    origin: u16,
    cols: u16,
) -> Vec<Button> {
    let mut result = Vec::new();
    let mut row = 1;
    for line in &lines[start..end] {
        let mut x = origin as usize;
        for span in &line.spans {
            let right = x + span.width() * line.physical_height();
            if let Some(code) = span.code_copy
                && right <= cols.saturating_sub(1) as usize
            {
                result.push(Button {
                    code,
                    row,
                    left: x as u16,
                    right: right as u16,
                });
            }
            x = right;
        }
        row += line.physical_height() as u16;
    }
    result
}

#[derive(Default)]
pub(crate) struct CopyButtons {
    armed: Option<Button>,
    feedback: Option<(usize, bool, Instant)>,
}
impl CopyButtons {
    pub fn event(
        &mut self,
        event: &Event,
        buttons: &[Button],
        codes: &[CodeSnippet],
        copy: &mut impl FnMut(&str) -> Result<(), CopyError>,
        now: Instant,
    ) -> Option<String> {
        let Event::Mouse(mouse) = event else {
            self.armed = None;
            return None;
        };
        let hit = buttons
            .iter()
            .find(|b| b.row == mouse.row && b.left <= mouse.column && mouse.column < b.right)
            .copied();
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if mouse.modifiers.is_empty() => {
                self.armed = hit
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pressed = self.armed.take()?;
                if !mouse.modifiers.is_empty() || hit != Some(pressed) {
                    return None;
                }
                let code = codes.get(pressed.code)?;
                let result = if code.text.is_empty() {
                    Err(CopyError::Empty)
                } else {
                    copy(&code.text)
                };
                self.feedback = Some((pressed.code, result.is_ok(), now + Duration::from_secs(3)));
                return Some(match result {
                    Ok(()) => "已复制代码块".into(),
                    Err(error) => error.to_string(),
                });
            }
            MouseEventKind::Moved => {}
            _ => self.armed = None,
        }
        None
    }
    pub fn expire(&mut self, now: Instant) -> bool {
        if self
            .feedback
            .is_some_and(|(_, _, deadline)| now >= deadline)
        {
            self.feedback = None;
            true
        } else {
            false
        }
    }
    pub fn decorate(&self, line: &Line, now: Instant) -> Option<Line> {
        let (code, success, deadline) = self.feedback?;
        if now >= deadline || !line.spans.iter().any(|s| s.code_copy == Some(code)) {
            return None;
        }
        let mut marked = line.clone();
        for span in &mut marked.spans {
            if span.code_copy == Some(code) {
                span.text = match (success, span.width()) {
                    (true, 6) => "Copied",
                    (false, 6) => "Retry ",
                    (true, _) => "Done",
                    (false, _) => "Fail",
                }
                .into();
            }
        }
        Some(marked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
    use tmd_core::{Document, RenderOptions, Theme};
    fn mouse(kind: MouseEventKind, x: u16, y: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    }
    #[test]
    fn click_copies_exact_code_once_and_keeps_layout() {
        let doc = Document::new("```text\n\t中文  \n\nlast\n```", Theme::default());
        let lines = doc.layout(
            30,
            RenderOptions {
                code_copy_buttons: true,
                ..Default::default()
            },
        );
        let controls = buttons(&lines, 0, lines.len(), 0, 80);
        assert_eq!(controls.len(), 1);
        let b = controls[0];
        let mut state = CopyButtons::default();
        let now = Instant::now();
        let mut received = Vec::new();
        let mut copy = |s: &str| {
            received.push(s.to_owned());
            Ok(())
        };
        assert!(
            state
                .event(
                    &mouse(MouseEventKind::Down(MouseButton::Left), b.left, b.row),
                    &controls,
                    doc.code_blocks(),
                    &mut copy,
                    now
                )
                .is_none()
        );
        assert_eq!(
            state
                .event(
                    &mouse(MouseEventKind::Up(MouseButton::Left), b.left, b.row),
                    &controls,
                    doc.code_blocks(),
                    &mut copy,
                    now
                )
                .as_deref(),
            Some("已复制代码块")
        );
        assert!(
            state
                .event(
                    &mouse(MouseEventKind::Up(MouseButton::Left), b.left, b.row),
                    &controls,
                    doc.code_blocks(),
                    &mut copy,
                    now
                )
                .is_none()
        );
        assert_eq!(received, ["\t中文  \n\nlast\n"]);
        let feedback = state.decorate(&lines[0], now).unwrap();
        assert!(feedback.plain_text().contains("Copied"));
        assert_eq!(feedback.width(), lines[0].width());
        assert!(
            state
                .decorate(&lines[0], now + Duration::from_secs(4))
                .is_none()
        );
    }
    #[test]
    fn hit_boxes_follow_visible_rows_and_sidebar_origin() {
        let doc = Document::new(
            "# Title\n\n```\nfirst\n```\n\n> ```\n> second\n> ```",
            Theme::default(),
        );
        let lines = doc.layout(
            40,
            RenderOptions {
                code_copy_buttons: true,
                large_headings: true,
            },
        );
        let full = buttons(&lines, 0, lines.len(), 0, 80);
        let shifted = buttons(&lines, 0, lines.len(), 31, 100);
        assert_eq!(full.len(), 2);
        for (a, b) in full.iter().zip(&shifted) {
            assert_eq!(b.left, a.left + 31);
            assert_eq!(b.right, a.right + 31);
            assert_eq!(b.row, a.row);
        }
        let second = lines
            .iter()
            .position(|l| l.spans.iter().any(|s| s.code_copy == Some(1)))
            .unwrap();
        let scrolled = buttons(&lines, second, lines.len(), 0, 80);
        assert_eq!(scrolled.len(), 1);
        assert_eq!((scrolled[0].code, scrolled[0].row), (1, 1));
        assert!(buttons(&lines, second + 1, lines.len(), 0, 80).is_empty());
        assert!(buttons(&lines, 0, lines.len(), 0, 4).is_empty());
    }

    #[test]
    fn release_must_hit_same_button_and_failure_can_retry() {
        let doc = Document::new("```\none\n```\n\n```\ntwo\n```", Theme::default());
        let lines = doc.layout(
            30,
            RenderOptions {
                code_copy_buttons: true,
                ..Default::default()
            },
        );
        let controls = buttons(&lines, 0, lines.len(), 0, 80);
        let b = controls[0];
        let now = Instant::now();
        let down = mouse(MouseEventKind::Down(MouseButton::Left), b.left, b.row);
        let up = mouse(MouseEventKind::Up(MouseButton::Left), b.left, b.row);
        let mut modified = up.clone();
        if let Event::Mouse(ref mut m) = modified {
            m.modifiers = KeyModifiers::SHIFT;
        }
        for released in [
            mouse(MouseEventKind::Up(MouseButton::Left), b.right, b.row),
            mouse(
                MouseEventKind::Up(MouseButton::Left),
                controls[1].left,
                controls[1].row,
            ),
            modified,
        ] {
            let mut state = CopyButtons::default();
            let mut forbidden = |_: &str| -> Result<(), CopyError> { panic!("unexpected copy") };
            state.event(&down, &controls, doc.code_blocks(), &mut forbidden, now);
            assert!(
                state
                    .event(&released, &controls, doc.code_blocks(), &mut forbidden, now)
                    .is_none()
            );
        }
        let mut state = CopyButtons::default();
        let mut failed = |_: &str| Err(CopyError::Backend("unavailable"));
        state.event(&down, &controls, doc.code_blocks(), &mut failed, now);
        assert!(
            state
                .event(&up, &controls, doc.code_blocks(), &mut failed, now)
                .unwrap()
                .contains("unavailable")
        );
        assert!(
            state
                .decorate(&lines[0], now)
                .unwrap()
                .plain_text()
                .contains("Retry")
        );
        let mut received = Vec::new();
        let mut copy = |s: &str| {
            received.push(s.to_owned());
            Ok(())
        };
        state.event(&down, &controls, doc.code_blocks(), &mut copy, now);
        state.event(&up, &controls, doc.code_blocks(), &mut copy, now);
        assert_eq!(received, ["one\n"]);
        assert!(!state.expire(now));
        assert!(state.expire(now + Duration::from_secs(4)));
        assert!(state.decorate(&lines[0], now).is_none());
    }

    #[test]
    fn cancelled_clicks_and_empty_blocks_do_not_write() {
        let doc = Document::new("```\n```\n\n```\nx\n```", Theme::default());
        let lines = doc.layout(
            30,
            RenderOptions {
                code_copy_buttons: true,
                ..Default::default()
            },
        );
        let controls = buttons(&lines, 0, lines.len(), 0, 80);
        assert_eq!(controls.len(), 2);
        let b = controls[1];
        let now = Instant::now();
        for cancel in [
            Event::Resize(50, 20),
            Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
            mouse(MouseEventKind::Drag(MouseButton::Left), b.left, b.row),
            mouse(MouseEventKind::ScrollDown, b.left, b.row),
        ] {
            let mut state = CopyButtons::default();
            let mut forbidden = |_: &str| -> Result<(), CopyError> { panic!("unexpected copy") };
            state.event(
                &mouse(MouseEventKind::Down(MouseButton::Left), b.left, b.row),
                &controls,
                doc.code_blocks(),
                &mut forbidden,
                now,
            );
            state.event(&cancel, &controls, doc.code_blocks(), &mut forbidden, now);
            assert!(
                state
                    .event(
                        &mouse(MouseEventKind::Up(MouseButton::Left), b.left, b.row),
                        &controls,
                        doc.code_blocks(),
                        &mut forbidden,
                        now
                    )
                    .is_none()
            );
        }
        let b = controls[0];
        let mut state = CopyButtons::default();
        let mut forbidden = |_: &str| -> Result<(), CopyError> { panic!("empty block copied") };
        state.event(
            &mouse(MouseEventKind::Down(MouseButton::Left), b.left, b.row),
            &controls,
            doc.code_blocks(),
            &mut forbidden,
            now,
        );
        assert!(
            state
                .event(
                    &mouse(MouseEventKind::Up(MouseButton::Left), b.left, b.row),
                    &controls,
                    doc.code_blocks(),
                    &mut forbidden,
                    now
                )
                .unwrap()
                .contains("为空")
        );
    }
}
