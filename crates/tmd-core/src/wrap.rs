//! Greedy word wrapping over styled spans.

use crate::line::Span;
use crate::style::Style;
use crate::width::{cluster_width, clusters, text_width};

/// Stands in for a character wider than the whole row (only possible when
/// the row is one column wide), so no row overflows.
const TOO_WIDE: &str = "…";

/// A row of spans being built.
pub(crate) type Row = Vec<Span>;

/// Appends `text` to `row`, merging into the last span when styles match.
pub(crate) fn push_text(row: &mut Row, text: &str, style: &Style) {
    if text.is_empty() {
        return;
    }
    match row.last_mut() {
        Some(last) if last.style == *style => last.text.push_str(text),
        _ => row.push(Span::new(text, style.clone())),
    }
}

fn row_width(row: &Row) -> usize {
    row.iter().map(Span::width).sum()
}

enum Token {
    /// Unbreakable non-space pieces, possibly in several styles. Two words
    /// in a row may be broken between with no space, as around CJK.
    Word {
        pieces: Vec<(String, Style)>,
        /// Whether following narrow text continues this word; false after a
        /// wide character, which stands alone.
        open: bool,
        /// Whether the word ends with opening punctuation and so must stay
        /// with whatever follows.
        glue: bool,
    },
    Space(Vec<(String, Style)>),
    Break,
}

/// Closing punctuation that must not start a row (Chinese line-breaking
/// rules); it stays with the text before it.
const NO_ROW_START: &str = "，。、；：！？）」』》〉】〕”’…—,.;:!?)]}";
/// Opening punctuation that must not end a row; it stays with the text
/// after it.
const NO_ROW_END: &str = "（「『《〈【〔“‘";

fn tokenize(spans: &[Span]) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    for span in spans {
        if span.text == "\n" {
            tokens.push(Token::Break);
            continue;
        }
        for piece in split_spaces(&span.text) {
            let entry = (piece.to_string(), span.style.clone());
            if piece.starts_with(' ') {
                match tokens.last_mut() {
                    Some(Token::Space(pieces)) => pieces.push(entry),
                    _ => tokens.push(Token::Space(vec![entry])),
                }
            } else {
                push_word_segments(&mut tokens, piece, &span.style);
            }
        }
    }
    tokens
}

/// Adds non-space text: each wide character becomes its own word so rows
/// can break around it; narrow runs join the open word before them, and
/// punctuation joins the side it belongs to.
fn push_word_segments(tokens: &mut Vec<Token>, text: &str, style: &Style) {
    for (segment, wide) in split_wide(text) {
        let (Some(first), Some(last)) = (segment.chars().next(), segment.chars().last()) else {
            continue;
        };
        let entry = (segment.to_string(), style.clone());
        let ends_open = NO_ROW_END.contains(last);
        match tokens.last_mut() {
            Some(Token::Word { pieces, open, glue })
                if *glue || NO_ROW_START.contains(first) || (*open && !wide) =>
            {
                pieces.push(entry);
                *open = !wide;
                *glue = ends_open;
            }
            _ => tokens.push(Token::Word {
                pieces: vec![entry],
                open: !wide,
                glue: ends_open,
            }),
        }
    }
}

/// Splits `text` into single wide clusters and runs of narrow ones; the
/// flag says whether a segment is a wide cluster.
fn split_wide(text: &str) -> Vec<(&str, bool)> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    for cluster in clusters(text) {
        let end = offset + cluster.len();
        if cluster_width(cluster) >= 2 {
            if start < offset {
                segments.push((&text[start..offset], false));
            }
            segments.push((cluster, true));
            start = end;
        }
        offset = end;
    }
    if start < text.len() {
        segments.push((&text[start..], false));
    }
    segments
}

/// Splits `text` into alternating runs of spaces and non-spaces.
fn split_spaces(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut in_space = None;
    for (i, c) in text.char_indices() {
        let space = c == ' ';
        if in_space.is_some_and(|prev| prev != space) {
            pieces.push(&text[start..i]);
            start = i;
        }
        in_space = Some(space);
    }
    if start < text.len() {
        pieces.push(&text[start..]);
    }
    pieces
}

fn pieces_width(pieces: &[(String, Style)]) -> usize {
    pieces.iter().map(|(text, _)| text_width(text)).sum()
}

/// Wraps styled spans into rows at most `width` columns wide. Words wider
/// than a row are broken between characters. A span whose text is `"\n"`
/// forces a break. Always returns at least one row.
pub(crate) fn wrap(spans: &[Span], width: usize) -> Vec<Row> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut row = Row::new();
    let mut pending_space: Vec<(String, Style)> = Vec::new();

    for token in tokenize(spans) {
        match token {
            Token::Break => {
                rows.push(std::mem::take(&mut row));
                pending_space.clear();
            }
            Token::Space(pieces) => {
                if !row.is_empty() {
                    pending_space.extend(pieces);
                }
            }
            Token::Word { pieces, .. } => {
                let word_width = pieces_width(&pieces);
                let fits = row_width(&row) + pieces_width(&pending_space) + word_width <= width;
                if fits {
                    for (text, style) in pending_space.drain(..) {
                        push_text(&mut row, &text, &style);
                    }
                } else {
                    pending_space.clear();
                    if !row.is_empty() {
                        rows.push(std::mem::take(&mut row));
                    }
                }
                if word_width <= width - row_width(&row) {
                    for (text, style) in &pieces {
                        push_text(&mut row, text, style);
                    }
                } else {
                    hard_break(&pieces, width, &mut row, &mut rows);
                }
            }
        }
    }
    rows.push(row);
    rows
}

/// Fills rows cluster by cluster with a word too wide for one row.
fn hard_break(pieces: &[(String, Style)], width: usize, row: &mut Row, rows: &mut Vec<Row>) {
    for (text, style) in pieces {
        for cluster in clusters(text) {
            let (cluster, w) = fit_cluster(cluster, width);
            let used = row_width(row);
            if used > 0 && used + w > width {
                rows.push(std::mem::take(row));
            }
            push_text(row, cluster, style);
        }
    }
}

/// `cluster` and its width, or a one-column stand-in when it is wider than
/// the whole row.
fn fit_cluster(cluster: &str, width: usize) -> (&str, usize) {
    let w = cluster_width(cluster);
    if w > width {
        (TOO_WIDE, text_width(TOO_WIDE))
    } else {
        (cluster, w)
    }
}

/// Breaks preformatted text into rows of at most `width` columns, one or
/// more rows per source line.
pub(crate) fn hard_wrap_lines(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for line in text.lines() {
        let mut current = String::new();
        let mut used = 0;
        for cluster in clusters(line) {
            let (cluster, w) = fit_cluster(cluster, width);
            if used > 0 && used + w > width {
                rows.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push_str(cluster);
            used += w;
        }
        rows.push(current);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|row| row.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    fn text(s: &str) -> Vec<Span> {
        vec![Span::new(s, Style::default())]
    }

    #[test]
    fn breaks_between_words() {
        let rows = wrap(&text("the quick brown fox"), 10);
        assert_eq!(plain(&rows), ["the quick", "brown fox"]);
    }

    #[test]
    fn hard_breaks_words_wider_than_the_row() {
        let rows = wrap(&text("ab abcdefghij"), 4);
        assert_eq!(plain(&rows), ["ab", "abcd", "efgh", "ij"]);
    }

    #[test]
    fn keeps_styled_pieces_of_one_word_together() {
        let bold = Style::default().bold();
        let spans = vec![
            Span::new("aaa ", Style::default()),
            Span::new("bb", bold.clone()),
            Span::new("cc", Style::default()),
        ];
        let rows = wrap(&spans, 6);
        assert_eq!(plain(&rows), ["aaa", "bbcc"]);
        assert_eq!(rows[1][0].style, bold);
    }

    #[test]
    fn newline_span_forces_a_break() {
        let spans = vec![
            Span::new("a", Style::default()),
            Span::new("\n", Style::default()),
            Span::new("b", Style::default()),
        ];
        assert_eq!(plain(&wrap(&spans, 80)), ["a", "b"]);
    }

    #[test]
    fn empty_input_gives_one_empty_row() {
        assert_eq!(plain(&wrap(&[], 10)), [""]);
    }

    #[test]
    fn hard_wrap_lines_keeps_empty_lines() {
        assert_eq!(hard_wrap_lines("abcde\n\nx", 3), ["abc", "de", "", "x"]);
    }
}
