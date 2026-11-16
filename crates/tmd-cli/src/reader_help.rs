//! Contextual help as a scannable shortcut reference, independent of document layout.
use crate::{i18n::text as t, reader_ui::Pane};
use tmd_core::{Line, Span, Style, Theme, clusters, text_width, truncate};

pub(crate) fn title(pane: Pane) -> &'static str {
    match pane {
        Pane::Reading => t("Reading help", "阅读帮助"),
        Pane::Outline => t("Outline help", "目录帮助"),
    }
}

pub(crate) fn lines(cols: u16, pane: Pane, theme: &Theme, fallback: &str) -> Vec<Line> {
    let mut help = Layout::new(cols, theme);
    match pane {
        Pane::Reading => {
            help.section(
                t("Search", "搜索"),
                &[
                    (
                        "/",
                        t(
                            "Search document text · case-sensitive",
                            "搜索正文，区分大小写",
                        ),
                    ),
                    ("Enter", t("Confirm the search", "确认搜索")),
                    ("n / N", t("Next / previous match", "下一处 / 上一处匹配")),
                    (
                        "Esc",
                        t(
                            "While typing: cancel and restore the previous search",
                            "输入时：取消，恢复上次搜索",
                        ),
                    ),
                    (
                        "Ctrl-U",
                        t("While typing: clear the query", "输入时：清空查询"),
                    ),
                ],
            );
            help.note(t(
                "While searching, letters are query text. Use F2 to switch language.",
                "搜索输入中，字母均作为查询文字；切换语言请用 F2。",
            ));
            help.section(
                t("Navigation", "导航"),
                &[
                    (
                        t("↑ ↓ / j k / Wheel", "↑ ↓ / j k / 滚轮"),
                        t("Scroll the document", "滚动正文"),
                    ),
                    ("PgUp / b", t("Previous page", "上一页")),
                    ("PgDn / Space", t("Next page", "下一页")),
                    ("Home / End", t("Beginning / end", "开头 / 末尾")),
                    (
                        "o",
                        t("Open or close the heading outline", "打开 / 关闭标题目录"),
                    ),
                    (
                        "Tab",
                        t(
                            "Switch focus between outline and document",
                            "切换目录与正文焦点",
                        ),
                    ),
                ],
            );
        }
        Pane::Outline => {
            help.section(
                t("Heading outline", "标题目录"),
                &[
                    (
                        "↑ ↓ / j k",
                        t(
                            "Select a heading without moving the document",
                            "选择标题，不移动正文",
                        ),
                    ),
                    ("Enter", t("Jump to the selected heading", "跳转到选中标题")),
                    ("Tab", t("Focus the document", "切换到正文")),
                    ("Esc / q / o", t("Close the outline", "关闭目录")),
                    (
                        t("Wheel", "滚轮"),
                        t("Scroll the pane under the pointer", "滚动指针所在区域"),
                    ),
                    (
                        "PgUp / PgDn",
                        t(
                            "Previous / next page; b / Space also work",
                            "上一页 / 下一页，也可用 b / 空格",
                        ),
                    ),
                    ("Home / End", t("First / last heading", "首个 / 末个标题")),
                ],
            );
            help.note(t("In a sidebar, Enter keeps outline focus. In a narrow window, it returns to the document.",
                "侧栏中 Enter 保留目录焦点；窄窗口中跳转后返回正文。"));
            help.section(
                t("Outline markers", "目录标记"),
                &[
                    (t("Background", "背景"), t("Selected heading", "选中项")),
                    ("▎", t("Section currently being read", "当前阅读章节")),
                    (
                        "L123",
                        t("Heading source line number", "标题所在的源码行号"),
                    ),
                ],
            );
            help.note(t(
                "Jumps keep the search. Repeated or untitled headings have distinct targets.",
                "目录跳转保留搜索；重复标题和空标题各有独立位置。",
            ));
        }
    }
    help.section(
        t("Copy", "复制"),
        &[
            (
                t("Drag text", "拖选文字"),
                t(
                    "Release the left button to copy the selection",
                    "松开左键，自动复制所选文字",
                ),
            ),
            (
                t("Copy button", "“复制”按钮"),
                t(
                    "Copy the full original code (top right)",
                    "代码块右上角：复制完整原始代码",
                ),
            ),
        ],
    );
    help.section(
        t("General", "通用操作"),
        &[
            (
                "L / F2",
                t(
                    "Switch English / Chinese for this session",
                    "切换中英文，仅当前会话",
                ),
            ),
            (
                "? / Esc / q",
                t(
                    "In help: return to the previous view",
                    "帮助中：返回之前的界面",
                ),
            ),
            ("q", t("In the document: quit reading", "正文中：退出阅读")),
            (
                "Esc",
                t(
                    "In the document: clear search, close outline, then quit",
                    "正文中：依次清除搜索、关闭目录、退出",
                ),
            ),
            (
                "Ctrl-C",
                t("Interrupt and return to the shell", "中断并返回命令行"),
            ),
        ],
    );
    if !fallback.is_empty() {
        help.section(t("Reading mode", "阅读模式"), &[]);
        help.note(fallback);
    }
    help.rows
}

struct Layout<'a> {
    rows: Vec<Line>,
    width: usize,
    inset: usize,
    theme: &'a Theme,
}
impl<'a> Layout<'a> {
    fn new(cols: u16, theme: &'a Theme) -> Self {
        let available = cols.saturating_sub(1) as usize;
        let inset = if cols >= 40 { 2 } else { 0 };
        Self {
            rows: Vec::new(),
            width: available.saturating_sub(inset).min(88),
            inset,
            theme,
        }
    }
    fn push(&mut self, spans: Vec<Span>) {
        let mut row = crate::reader_ui::line("");
        row.spans = vec![Span::new(" ".repeat(self.inset), self.theme.text.clone())];
        row.spans.extend(spans);
        self.rows.push(row);
    }
    fn section(&mut self, title: &str, entries: &[(&str, &str)]) {
        if !self.rows.is_empty() {
            self.rows.push(crate::reader_ui::line(""));
        }
        for part in wrap(title, self.width) {
            self.push(vec![Span::new(part, self.theme.h2.clone().bold())]);
        }
        // A stable column across groups makes the reference easy to scan.
        let key_width = 20;
        let gap = 3;
        let two_columns = self.width >= 56;
        for &(key, description) in entries {
            let key_style = self.theme.text.clone().bold();
            if two_columns {
                let descriptions = wrap(description, self.width - key_width - gap);
                for (index, part) in descriptions.into_iter().enumerate() {
                    let label = if index == 0 { key } else { "" };
                    self.push(vec![
                        Span::new(label, key_style.clone()),
                        Span::new(
                            " ".repeat(key_width + gap - text_width(label)),
                            self.theme.text.clone(),
                        ),
                        Span::new(part, self.theme.text.clone()),
                    ]);
                }
            } else {
                for part in wrap(key, self.width) {
                    self.push(vec![Span::new(part, key_style.clone())]);
                }
                let indent = 2.min(self.width.saturating_sub(1));
                for part in wrap(description, self.width.saturating_sub(indent)) {
                    self.push(vec![Span::new(
                        format!("{}{part}", " ".repeat(indent)),
                        self.theme.text.clone(),
                    )]);
                }
            }
        }
    }
    fn note(&mut self, text: &str) {
        // Secondary explanation stays readable in light and dark themes.
        for part in wrap(text, self.width) {
            self.push(vec![Span::new(
                part,
                Style {
                    fg: self.theme.muted.fg,
                    ..self.theme.text.clone()
                },
            )]);
        }
    }
}

/// Prefer word boundaries for prose, retaining the core's cluster-safe fallback.
fn wrap(mut text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }
    let mut rows = Vec::new();
    while !text.is_empty() {
        let prefix = truncate(text, width);
        if prefix.is_empty() {
            rows.push("…".into());
            let cluster = clusters(text).next().expect("nonempty text");
            text = &text[cluster.len()..];
            continue;
        }
        let split = if prefix.len() < text.len() {
            prefix
                .rfind(' ')
                .filter(|&i| text_width(&prefix[..i]) >= width / 2)
        } else {
            None
        };
        let len = split.unwrap_or(prefix.len());
        rows.push(text[..len].to_string());
        text = text[len..].trim_start();
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, scoped};

    #[test]
    fn every_help_view_fits_both_languages_and_theme_widths() {
        for language in [Language::English, Language::Chinese] {
            let _guard = scoped(language);
            for name in Theme::builtin_names() {
                let theme = Theme::builtin(name).unwrap();
                for pane in [Pane::Reading, Pane::Outline] {
                    for cols in 0..=140 {
                        let rows = lines(cols, pane, &theme, "Native fallback · 原生定位不可用");
                        assert!(
                            rows.iter()
                                .all(|row| row.width() <= cols.saturating_sub(1) as usize),
                            "{language:?} {name} {pane:?} {cols}"
                        );
                        if cols >= 20 {
                            assert!(!rows.is_empty());
                            assert!(
                                rows.iter()
                                    .any(|row| row.spans.iter().any(|span| span.style.bold))
                            );
                            assert!(rows.iter().any(|row| row.plain_text().is_empty()));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn wide_help_aligns_descriptions_and_keeps_shortcuts_complete() {
        let _guard = scoped(Language::English);
        let theme = Theme::default();
        let wide = lines(100, Pane::Reading, &theme, "");
        for key in [
            "/",
            "Enter",
            "n / N",
            "Ctrl-U",
            "PgUp / b",
            "PgDn / Space",
            "L / F2",
            "Ctrl-C",
        ] {
            let row = wide
                .iter()
                .find(|row| {
                    row.spans
                        .iter()
                        .any(|span| span.text == key && span.style.bold)
                })
                .unwrap();
            assert_eq!(row.spans[..3].iter().map(Span::width).sum::<usize>(), 25);
            assert_eq!(row.spans[0].width(), 2);
        }
        let text = wide
            .iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Release the left button"));
        assert!(text.contains("Copy button"));
        assert!(!text.contains("Shift"));
        let narrow = lines(39, Pane::Reading, &theme, "");
        assert!(narrow.len() > wide.len());
        let enter = narrow
            .iter()
            .position(|row| row.plain_text() == "Enter")
            .unwrap();
        assert_eq!(narrow[enter + 1].plain_text(), "  Confirm the search");
    }

    #[test]
    fn prose_wrap_uses_words_and_preserves_character_clusters() {
        assert_eq!(
            wrap("Search document text", 16),
            ["Search document", "text"]
        );
        let text = "中文e\u{301}👩‍💻尾";
        assert_eq!(wrap(text, 4).join(""), text);
        assert!(wrap(text, 4).iter().all(|line| text_width(line) <= 4));
    }
}
