//! Small presentation/state helpers shared by the reader's transient views.
use std::time::{Duration, Instant};
use tmd_core::{Line, LineSize, Span, Style, clusters, text_width, truncate};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pane {
    #[default]
    Reading,
    Outline,
    Copy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum View {
    #[default]
    Reading,
    Search,
    Outline,
    Copy,
    Help(Pane),
}

impl Pane {
    pub fn view(self) -> View {
        match self {
            Self::Reading => View::Reading,
            Self::Outline => View::Outline,
            Self::Copy => View::Copy,
        }
    }
}

#[derive(Default)]
pub(crate) struct ListViewport {
    top: usize,
    center: bool,
}

impl ListViewport {
    pub fn reset(&mut self) {
        self.center = true;
    }

    pub fn top(&mut self, selected: usize, count: usize, height: usize) -> usize {
        let height = height.max(1);
        if self.center {
            self.top = selected.saturating_sub(height / 2);
            self.center = false;
        } else if selected < self.top {
            self.top = selected;
        } else if selected >= self.top.saturating_add(height) {
            self.top = selected.saturating_sub(height - 1);
        }
        self.top = self.top.min(count.saturating_sub(height));
        self.top
    }
}

pub(crate) fn line(text: impl Into<String>) -> Line {
    Line {
        spans: vec![Span::new(text, Style::default())],
        src: 0..0,
        size: LineSize::Normal,
    }
}

pub(crate) fn ellipsize(text: &str, width: usize) -> String {
    if text_width(text) <= width {
        text.into()
    } else if width == 0 {
        String::new()
    } else {
        format!("{}…", truncate(text, width - 1))
    }
}

/// Wrap help/details at grapheme boundaries; never wrap a wide cluster into a
/// narrower terminal. The last limited row shows an explicit omission marker.
pub(crate) fn wrapped(text: &str, width: usize, limit: usize) -> Vec<Line> {
    if width == 0 || limit == 0 {
        return Vec::new();
    }
    let mut rows = Vec::new();
    let mut rest = text;
    while !rest.is_empty() && rows.len() < limit {
        if rows.len() + 1 == limit {
            rows.push(line(ellipsize(rest, width)));
            break;
        }
        let part = truncate(rest, width);
        if part.is_empty() {
            // Only possible for a cluster wider than this very small viewport.
            let cluster = clusters(rest).next().expect("non-empty text");
            rows.push(line("…"));
            rest = &rest[cluster.len()..];
        } else {
            rows.push(line(part));
            rest = &rest[part.len()..];
        }
    }
    if rows.is_empty() {
        rows.push(line(""));
    }
    rows
}

/// Reserve whole action labels before allocating the remaining cells to status.
pub(crate) fn footer(view: View, state: &str, query: bool, cols: u16) -> String {
    let width = cols.saturating_sub(1) as usize;
    let variants: &[&str] = match view {
        View::Reading if query => &[
            "n 下一处  N 上一处  Esc 清除  ? 帮助  q 退出",
            "n/N 跳转  Esc 清除  q 退出",
            "Esc 清除  q 退出",
            "Esc  q",
            "q",
        ],
        View::Reading => &[
            "/ 搜索  o 目录  c 复制  ? 帮助  q 退出",
            "o 目录  ? 帮助  q 退出",
            "? 帮助  q 退出",
            "?  q",
            "q",
        ],
        View::Outline => &[
            "↑↓ 选择  Enter 跳转  Esc 返回  ? 帮助",
            "Enter 跳转  Esc 返回  ? 帮助",
            "Enter 跳转  Esc 返回",
            "Enter  Esc",
            "Esc",
        ],
        View::Copy => &[
            "↑↓ 选择  Enter 复制  Esc 返回  ? 帮助",
            "Enter 复制  Esc 返回  ? 帮助",
            "Enter 复制  Esc 返回",
            "Enter  Esc",
            "Esc",
        ],
        View::Help(_) => &["↑↓ 滚动  Esc 返回", "Esc 返回", "Esc"],
        View::Search => &["Enter 搜索  Esc 取消", "Enter  Esc", "Esc"],
    };
    let skip = if view == View::Search {
        usize::from(cols < 40)
    } else if cols < 20 {
        variants.len().saturating_sub(2)
    } else if cols < 40 {
        2.min(variants.len() - 1)
    } else if cols < 80 {
        1.min(variants.len() - 1)
    } else {
        0
    };
    let actions = variants[skip..]
        .iter()
        .copied()
        .find(|s| text_width(s) <= width)
        .unwrap_or("");
    let remaining = width.saturating_sub(text_width(actions));
    let state = ellipsize(state, remaining.saturating_sub(2));
    if state.is_empty() {
        actions.into()
    } else {
        format!(
            "{state}{}{actions}",
            " ".repeat(remaining - text_width(&state))
        )
    }
}

/// Split panes only appear at >=80 columns, leaving room for focus controls.
pub(crate) fn sidebar_footer(view: View, state: &str, query: bool, cols: u16) -> String {
    let actions = match view {
        View::Outline => "↑↓ 选择  Enter 跳转  Tab 正文  Esc 关闭  ? 帮助",
        View::Reading if query => "n/N 跳转  Esc 清除  Tab 目录  o 关闭  ? 帮助  q 退出",
        _ => "↑↓ 滚动  Tab 目录  / 搜索  c 复制  o 关闭  ? 帮助  q 退出",
    };
    let width = cols.saturating_sub(1) as usize;
    let remaining = width.saturating_sub(text_width(actions));
    let state = ellipsize(state, remaining.saturating_sub(2));
    format!(
        "{state}{}{actions}",
        " ".repeat(remaining - text_width(&state))
    )
}

#[derive(Default)]
pub(crate) struct Notice {
    value: Option<(String, Instant)>,
}

impl Notice {
    pub fn show(&mut self, text: impl Into<String>, now: Instant) {
        self.value = Some((text.into(), now + Duration::from_secs(3)));
    }

    pub fn clear(&mut self) {
        self.value = None;
    }

    pub fn text(&self, now: Instant) -> Option<&str> {
        self.value
            .as_ref()
            .filter(|(_, deadline)| now < *deadline)
            .map(|(text, _)| text.as_str())
    }

    pub fn expire(&mut self, now: Instant) -> bool {
        if self.value.as_ref().is_some_and(|(_, end)| now >= *end) {
            self.clear();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_moves_without_scrolling_inside_viewport() {
        let mut list = ListViewport::default();
        list.reset();
        assert_eq!(list.top(50, 100, 10), 45);
        assert_eq!(list.top(49, 100, 10), 45);
        assert_eq!(list.top(54, 100, 10), 45);
        assert_eq!(list.top(55, 100, 10), 46);
        assert_eq!(list.top(54, 100, 10), 46);
        assert_eq!(list.top(45, 100, 10), 45);
        assert_eq!(list.top(99, 100, 10), 90);
        assert_eq!(list.top(99, 100, 3), 97);
        assert_eq!(list.top(99, 100, 0), 99);
    }

    #[test]
    fn footer_keeps_actions_whole_and_only_advertises_active_commands() {
        for cols in [0, 1, 4, 20, 24, 40, 60, 80, 120] {
            for view in [
                View::Reading,
                View::Outline,
                View::Copy,
                View::Help(Pane::Reading),
            ] {
                let text = footer(view, &"很长的状态👩‍💻é".repeat(20), false, cols);
                assert!(text_width(&text) <= cols.saturating_sub(1) as usize);
                if view != View::Reading {
                    assert!(!text.contains("/ 搜索") && !text.contains("c 复制"));
                    if cols >= 20 {
                        assert!(text.contains("Esc"));
                    }
                }
                if matches!(view, View::Outline | View::Copy) && cols >= 24 {
                    assert!(text.contains("Enter"));
                }
            }
        }
        let text = footer(View::Reading, "匹配 2/8", true, 40);
        assert!(text.contains("Esc 清除") && text.contains("q 退出"));
        let wide = footer(View::Reading, "阅读 · 开头", false, 80);
        for key in ["/ 搜索", "o 目录", "c 复制", "? 帮助", "q 退出"] {
            assert!(wide.contains(key));
        }
    }

    #[test]
    fn sidebar_footer_reserves_focus_and_close_actions() {
        for width in [80, 100, 120, 200] {
            for query in [false, true] {
                for view in [View::Reading, View::Outline] {
                    let text = sidebar_footer(view, &"状态".repeat(100), query, width);
                    assert!(text_width(&text) < width as usize);
                    assert!(text.contains("Tab") && text.contains("关闭"));
                    if view == View::Outline {
                        assert!(text.contains("Enter"));
                    }
                }
            }
        }
    }

    #[test]
    fn messages_expire_without_an_input_event() {
        let now = Instant::now();
        let mut notice = Notice::default();
        notice.show("已复制", now);
        assert_eq!(notice.text(now + Duration::from_secs(2)), Some("已复制"));
        assert!(!notice.expire(now + Duration::from_secs(2)));
        assert!(notice.expire(now + Duration::from_secs(3)));
        assert_eq!(notice.text(now), None);
        notice.show("新提示", now);
        notice.clear();
        assert_eq!(notice.text(now), None);
    }

    #[test]
    fn details_wrap_without_splitting_clusters() {
        let text = "中é👩‍💻尾";
        let rows = wrapped(text, 3, 10);
        assert_eq!(rows.iter().map(Line::plain_text).collect::<String>(), text);
        assert!(rows.iter().all(|row| row.width() <= 3));
        assert!(wrapped(text, 3, 1)[0].plain_text().ends_with('…'));
    }

    #[test]
    fn search_instructions_reserve_both_actions_in_the_header() {
        for cols in [20, 24, 40, 80, 120] {
            let text = footer(View::Search, "搜索", false, cols);
            assert!(text.contains("Enter") && text.contains("Esc"));
            assert!(text_width(&text) < cols as usize);
        }
    }
}
