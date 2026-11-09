//! Heading selection retains source identity independently of list scrolling.
use crate::reader_ui::{ListViewport, ellipsize, line, wrapped};
use tmd_core::{Color, Heading, Line, Span, Style, Theme, text_width};

#[derive(Default)]
pub(crate) struct Outline {
    pub selected: usize,
    pub current: Option<usize>,
    viewport: ListViewport,
}

impl Outline {
    pub fn show(&mut self, headings: &[Heading], source_start: usize) -> bool {
        self.current = headings
            .iter()
            .rposition(|h| h.source.start <= source_start);
        self.selected = self.current.unwrap_or(0);
        self.viewport.reset();
        !headings.is_empty()
    }

    pub fn rows(&self, headings: &[Heading], cols: u16, theme: &Theme, focused: bool) -> Vec<Line> {
        let width = cols.saturating_sub(1) as usize;
        headings
            .iter()
            .enumerate()
            .map(|(index, heading)| {
                let mark = if Some(index) == self.current {
                    "▎"
                } else {
                    " "
                };
                let indent = if cols >= 60 {
                    "  ".repeat(heading.level.saturating_sub(1) as usize)
                } else {
                    " ".repeat(heading.level.saturating_sub(1).min(3) as usize)
                };
                let level = if cols >= 40 {
                    format!("H{} ", heading.level)
                } else {
                    String::new()
                };
                let suffix = if cols >= 40 {
                    format!(" · L{}", heading.source_line)
                } else {
                    String::new()
                };
                let prefix = format!("{mark} {indent}{level}");
                let available = width.saturating_sub(text_width(&prefix) + text_width(&suffix));
                let mut row = line(ellipsize(
                    &format!("{prefix}{}{suffix}", ellipsize(title(heading), available)),
                    width,
                ));
                let selected = index == self.selected;
                let background = if selected && focused {
                    match theme.code_block.bg {
                        Color::Reset => Color::Ansi(if theme.dark { 8 } else { 7 }),
                        color => color,
                    }
                } else {
                    theme.text.bg
                };
                let style = Style {
                    fg: if selected && !focused {
                        theme.muted.fg
                    } else {
                        theme.text.fg
                    },
                    bg: background,
                    ..Style::default()
                };
                let text = row.plain_text();
                let padded = format!(
                    "{text}{}",
                    " ".repeat(width.saturating_sub(text_width(&text)))
                );
                row.spans = if Some(index) == self.current && width > 0 {
                    vec![
                        Span::new(
                            "▎",
                            Style {
                                fg: theme.h2.fg,
                                ..style.clone()
                            },
                        ),
                        Span::new(&padded["▎".len()..], style),
                    ]
                } else {
                    vec![Span::new(padded, style)]
                };
                row
            })
            .collect()
    }

    pub fn details(&self, headings: &[Heading], size: (u16, u16)) -> Vec<Line> {
        let Some(heading) = headings.get(self.selected) else {
            return Vec::new();
        };
        if size.1 < 8 {
            return Vec::new();
        }
        let width = size.0.saturating_sub(1) as usize;
        // Keep at least four list rows; show extra title context only when useful.
        if text_width(title(heading)) + 24 <= width {
            return Vec::new();
        }
        wrapped(
            &format!("L{} · {}", heading.source_line, title(heading)),
            width,
            2,
        )
    }

    pub fn top(&mut self, count: usize, height: usize) -> usize {
        self.viewport.top(self.selected, count, height)
    }

    pub fn move_by(&mut self, amount: isize, count: usize) {
        self.selected = self
            .selected
            .saturating_add_signed(amount)
            .min(count.saturating_sub(1));
    }
}

pub(crate) fn title(heading: &Heading) -> &str {
    if heading.title.is_empty() {
        "（空标题）"
    } else {
        &heading.title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmd_core::{Document, Theme};

    #[test]
    fn current_section_and_selection_are_independent() {
        let doc = Document::new(
            "intro\n\n# Same\n\nbody\n\n## Same\n\n###\n",
            Theme::default(),
        );
        let mut outline = Outline::default();
        assert!(outline.show(doc.headings(), 0));
        assert_eq!(outline.current, None);
        outline.show(doc.headings(), doc.headings()[1].source.start);
        outline.move_by(1, doc.headings().len());
        assert_eq!(outline.current, Some(1));
        assert_eq!(outline.selected, 2);
        let rows = outline.rows(doc.headings(), 80, &Theme::default(), true);
        assert!(rows[1].plain_text().starts_with("▎ "));
        assert!(rows[2].plain_text().starts_with("  "));
        assert!(rows[2].plain_text().contains("（空标题）"));
        for width in [0, 1, 4, 12, 24, 40, 80] {
            assert!(
                outline
                    .rows(doc.headings(), width, &Theme::default(), true)
                    .iter()
                    .all(|row| row.width() <= width.saturating_sub(1) as usize)
            );
        }
    }

    #[test]
    fn long_titles_keep_source_line_and_details() {
        let doc = Document::new(&format!("###### {}", "中文👩‍💻".repeat(50)), Theme::default());
        let mut outline = Outline::default();
        outline.show(doc.headings(), 0);
        let row = outline.rows(doc.headings(), 40, &Theme::default(), true)[0].plain_text();
        assert!(row.trim_end().ends_with(" · L1"));
        assert!(row.contains('…'));
        assert_eq!(outline.details(doc.headings(), (40, 10)).len(), 2);
        assert!(outline.details(doc.headings(), (40, 7)).is_empty());
    }

    #[test]
    fn selection_fills_the_row_and_current_marker_survives_focus_changes() {
        for name in Theme::builtin_names() {
            let theme = Theme::builtin(name).unwrap();
            let doc = Document::new("# 中文👩‍💻\n\n## Other\n", theme.clone());
            let mut outline = Outline::default();
            outline.show(doc.headings(), 0);
            outline.move_by(1, 2);
            for width in [0, 1, 2, 4, 24, 40, 80] {
                let active = outline.rows(doc.headings(), width, &theme, true);
                let inactive = outline.rows(doc.headings(), width, &theme, false);
                assert_eq!(active[1].width(), width.saturating_sub(1) as usize);
                assert_eq!(active[0].plain_text(), inactive[0].plain_text());
                assert_eq!(active[1].plain_text(), inactive[1].plain_text());
                assert!(active[1].spans.iter().all(|span| {
                    span.style.bg == theme.code_block.bg
                        && !span.style.underline
                        && !span.style.bold
                }));
                assert!(
                    inactive[1]
                        .spans
                        .iter()
                        .all(|span| span.style.bg == theme.text.bg)
                );
                if width > 1 {
                    assert!(active[0].plain_text().starts_with('▎'));
                    assert!(!active[1].plain_text().contains('▎'));
                }
            }
        }
    }
}
