//! Code chooser state. Backend injection keeps tests away from real clipboards.
use crate::clipboard::CopyError;
use tmd_core::{CodeSnippet, Line, LineSize, Span, Style, clusters, text_width, truncate};

#[derive(Default)]
pub(crate) struct CodePicker {
    pub open: bool,
    pub selected: usize,
    pub notice: String,
    entries: Vec<String>,
}
impl CodePicker {
    /// `following` starts at the current reading row and includes the tail.
    pub fn show(&mut self, codes: &[CodeSnippet], following: &[Line]) {
        self.notice.clear();
        self.open = false;
        if codes.is_empty() {
            self.notice = " · 未找到代码块".into();
            return;
        }
        self.selected = following
            .iter()
            .flat_map(|line| &line.spans)
            .find_map(|span| span.code_block)
            .unwrap_or(codes.len() - 1);
        self.entries = codes
            .iter()
            .enumerate()
            .map(|(i, code)| {
                format!(
                    "{} · {} · L{} · {}",
                    i + 1,
                    code.language.as_deref().unwrap_or("代码"),
                    code.source_line,
                    preview(&code.text)
                )
            })
            .collect();
        self.open = true;
    }
    pub fn rows(&self, width: u16) -> Vec<Line> {
        self.entries
            .iter()
            .map(|text| Line {
                spans: vec![Span::new(
                    truncate(text, width.saturating_sub(1) as usize),
                    Style::default(),
                )],
                src: 0..0,
                size: LineSize::Normal,
            })
            .collect()
    }
    pub fn top(&self, height: usize) -> usize {
        self.selected.saturating_sub(height.saturating_sub(1))
    }
    pub fn move_by(&mut self, amount: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(amount)
            .min(self.entries.len().saturating_sub(1));
    }
    pub fn confirm(
        &mut self,
        codes: &[CodeSnippet],
        copy: &mut impl FnMut(&str) -> Result<(), CopyError>,
    ) {
        let Some(code) = codes.get(self.selected) else {
            return;
        };
        let result = if code.text.is_empty() {
            Err(CopyError::Empty)
        } else {
            copy(&code.text)
        };
        match result {
            Ok(()) => {
                self.notice = format!(" · 已复制代码块 {}", self.selected + 1);
                self.open = false;
            }
            Err(error) => self.notice = format!(" · {error}"),
        }
    }
}

fn preview(text: &str) -> String {
    let Some(line) = text.lines().find(|line| !line.trim().is_empty()) else {
        return "（空代码块）".into();
    };
    let mut out = String::new();
    let mut width = 0;
    for cluster in clusters(line) {
        let clean: String = cluster
            .chars()
            .filter_map(|c| {
                if c == '\t' {
                    Some(' ')
                } else if c.is_control() {
                    None
                } else {
                    Some(c)
                }
            })
            .collect();
        let used = text_width(&clean);
        if width + used > 100 {
            break;
        }
        out.push_str(&clean);
        width += used;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmd_core::{Document, RenderOptions, Theme};
    #[test]
    fn confirming_is_the_only_write_and_failure_keeps_the_selection() {
        let doc = Document::new(
            "```text\n\t中文  \n\nlast\n```\n\n```\n```",
            Theme::default(),
        );
        let lines = doc.layout(20, RenderOptions::default());
        let mut picker = CodePicker::default();
        picker.show(doc.code_blocks(), &lines);
        let mut calls = 0;
        picker.confirm(doc.code_blocks(), &mut |text| {
            calls += 1;
            assert_eq!(text, "\t中文  \n\nlast\n");
            Err(CopyError::Backend("测试写入失败"))
        });
        assert!(picker.open);
        assert_eq!(calls, 1);
        assert_eq!(picker.selected, 0);
        picker.confirm(doc.code_blocks(), &mut |_| {
            calls += 1;
            Ok(())
        });
        assert!(!picker.open);
        assert_eq!(calls, 2);
        assert!(picker.notice.contains("已复制"));
        picker.show(doc.code_blocks(), &lines);
        picker.move_by(1);
        picker.confirm(doc.code_blocks(), &mut |_| {
            panic!("empty block must not write")
        });
        assert!(picker.open);
        assert!(picker.notice.contains("为空"));
    }
    #[test]
    fn chooser_defaults_to_visible_code_and_previews_remove_controls() {
        let doc = Document::new("```\nfirst\n```\n\n```\nsecond\n```", Theme::default());
        let lines = doc.layout(20, RenderOptions::default());
        let row = Document::match_line(&lines, &doc.find("second")[0]).unwrap();
        let mut picker = CodePicker::default();
        picker.show(doc.code_blocks(), &lines[row..]);
        assert_eq!(picker.selected, 1);
        assert_eq!(preview("\x1b]52;bad\x07\t中文"), "]52;bad 中文");
    }

    #[test]
    fn chooser_recognizes_empty_and_control_only_blocks_from_their_label() {
        for container in ["", "> ", "  "] {
            let prefix = if container == "  " { "- item\n\n" } else { "" };
            let markdown = format!(
                "{prefix}{container}```first\n{container}one\n{container}```\n{container}\n\
                 {container}```empty\n{container}```\n{container}\n\
                 {container}```control\n{container}\x07\n{container}```\n{container}\n\
                 {container}```last\n{container}four\n{container}```\n"
            );
            let doc = Document::new(&markdown, Theme::default());
            assert_eq!(doc.code_blocks().len(), 4, "{markdown:?}");
            for width in [12, 80] {
                let lines = doc.layout(width, RenderOptions::default());
                for (label, expected) in [("empty", 1), ("control", 2)] {
                    let row = lines
                        .iter()
                        .position(|line| line.plain_text().ends_with(label))
                        .expect("code language label is visible");
                    for visible in [&lines[row..=row], &lines[row + 1..=row + 1]] {
                        let mut picker = CodePicker::default();
                        picker.show(doc.code_blocks(), visible);
                        assert_eq!(
                            picker.selected, expected,
                            "{container:?} {label} width={width}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn chooser_selects_the_next_empty_block_when_no_code_is_visible() {
        let doc = Document::new(
            "```\none\n```\n\nparagraph\n\n```\n```\n\n```\nlast\n```",
            Theme::default(),
        );
        let lines = doc.layout(20, RenderOptions::default());
        let row = Document::match_line(&lines, &doc.find("paragraph")[0]).unwrap();
        let mut picker = CodePicker::default();
        picker.show(doc.code_blocks(), &lines[row..]);
        assert_eq!(picker.selected, 1);
        // An unlabelled empty block still carries a visible identity.
        picker.show(doc.code_blocks(), &lines[row + 2..=row + 2]);
        assert_eq!(picker.selected, 1);
    }

    #[test]
    fn chooser_selects_next_code_after_a_viewport_of_only_decorations() {
        let markdown = format!("```\none\n```\n\n{}\n```\nthree\n```", "---\n\n".repeat(30));
        let doc = Document::new(&markdown, Theme::default());
        let lines = doc.layout(80, RenderOptions::default());
        let mut picker = CodePicker::default();
        picker.show(doc.code_blocks(), &lines[4..]);
        assert_eq!(picker.selected, 1);
    }
}
