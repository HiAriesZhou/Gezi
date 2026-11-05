//! Search state and Unicode-aware query editing, independent of terminal I/O.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::ops::Range;
use tmd_core::{Document, clusters, text_width};

#[derive(Default)]
pub(crate) struct Search {
    pub query: String,
    pub matches: Vec<Range<usize>>,
    pub selected: usize,
    pub input: Option<Query>,
    pub saved_start: usize,
}

impl Search {
    pub fn open(&mut self, start: usize) {
        self.saved_start = start;
        self.input = Some(Query {
            text: self.query.clone(),
            cursor: self.query.len(),
        });
    }
    pub fn confirm(&mut self, doc: &Document) -> bool {
        let Some(input) = self.input.take() else {
            return false;
        };
        if input.text.trim().is_empty() {
            return false;
        }
        self.query = input.text;
        self.matches = doc.find(&self.query);
        self.selected = 0;
        true
    }
    pub fn current(&self) -> Option<&Range<usize>> {
        self.matches.get(self.selected)
    }
    pub fn advance(&mut self, backward: bool) {
        let count = self.matches.len();
        if count > 0 {
            self.selected = if backward {
                (self.selected + count - 1) % count
            } else {
                (self.selected + 1) % count
            };
        }
    }
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.selected = 0;
    }
    pub fn notice(&self) -> String {
        if self.query.is_empty() {
            String::new()
        } else if self.matches.is_empty() {
            " · 未找到匹配".into()
        } else {
            format!(
                " · 匹配 {}/{} · n/N 前后",
                self.selected + 1,
                self.matches.len()
            )
        }
    }
}

pub(crate) struct Query {
    pub text: String,
    pub cursor: usize,
}

impl Query {
    pub fn insert(&mut self, text: &str) {
        let clean: String = text
            .chars()
            .filter_map(|c| match c {
                '\r' | '\n' | '\t' => Some(' '),
                c if c.is_control() => None,
                c => Some(c),
            })
            .collect();
        if clean.is_empty() {
            return;
        }
        self.text.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
        self.snap_cursor();
    }

    fn snap_cursor(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Inserting/deleting a joiner or separator can merge neighboring clusters.
        let mut end = 0;
        for cluster in clusters(&self.text) {
            end += cluster.len();
            if end >= self.cursor {
                self.cursor = end;
                break;
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent) {
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('u') {
            self.text.clear();
            self.cursor = 0;
            return;
        }
        if key.modifiers.intersects(
            KeyModifiers::CONTROL
                | KeyModifiers::ALT
                | KeyModifiers::SUPER
                | KeyModifiers::HYPER
                | KeyModifiers::META,
        ) {
            return;
        }
        match key.code {
            KeyCode::Char(c) => self.insert(&c.to_string()),
            KeyCode::Left => self.cursor = self.previous(),
            KeyCode::Right => {
                self.cursor += clusters(&self.text[self.cursor..])
                    .next()
                    .map_or(0, str::len)
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            KeyCode::Backspace => {
                let start = self.previous();
                self.text.drain(start..self.cursor);
                self.cursor = start;
            }
            KeyCode::Delete => {
                let end = self.cursor
                    + clusters(&self.text[self.cursor..])
                        .next()
                        .map_or(0, str::len);
                self.text.drain(self.cursor..end);
            }
            _ => {}
        }
        self.snap_cursor();
    }
    fn previous(&self) -> usize {
        self.cursor
            - clusters(&self.text[..self.cursor])
                .last()
                .map_or(0, str::len)
    }
    /// Horizontally scroll to keep a real terminal cursor inside the input row.
    pub fn visible(&self, columns: usize) -> (&str, usize) {
        let mut start = 0;
        let mut used = text_width(&self.text[..self.cursor]);
        while used >= columns.max(1) {
            let Some(cluster) = clusters(&self.text[start..self.cursor]).next() else {
                break;
            };
            used = used.saturating_sub(text_width(cluster));
            start += cluster.len();
        }
        let shown = tmd_core::truncate(&self.text[start..], columns);
        (shown, text_width(&self.text[start..self.cursor]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_edits_whole_clusters_and_paste_is_text() {
        let mut q = Query {
            text: String::new(),
            cursor: 0,
        };
        q.insert("é 👩‍💻 👍🏽 中文\nq\x1b");
        assert_eq!(q.text, "é 👩‍💻 👍🏽 中文 q");
        q.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        q.key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
        assert!(q.text.starts_with(' '));
        q.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        q.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        q.key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(!q.text.contains('👩'));
        let (_, cursor) = q.visible(2);
        assert!(cursor < 2);
    }
    #[test]
    fn large_queries_scroll_without_splitting_or_changing_the_cursor() {
        let mut q = Query {
            text: "中".repeat(100_000),
            cursor: 300_000,
        };
        let (shown, cursor) = q.visible(8);
        assert_eq!(shown, "中中中");
        assert_eq!(cursor, 6);
        q.cursor = 0;
        q.insert("\x1b");
        assert_eq!(q.cursor, 0);
    }

    #[test]
    fn search_counts_cycle_and_empty_query_keeps_previous_search() {
        let doc = Document::new("one one 中文", tmd_core::Theme::default());
        let mut s = Search::default();
        s.open(3);
        s.input.as_mut().unwrap().insert("one");
        assert!(s.confirm(&doc));
        assert_eq!(s.matches.len(), 2);
        s.advance(true);
        assert_eq!(s.selected, 1);
        s.advance(false);
        assert_eq!(s.selected, 0);
        s.open(5);
        s.input.as_mut().unwrap().text.clear();
        assert!(!s.confirm(&doc));
        assert_eq!(s.matches.len(), 2);
    }
}
