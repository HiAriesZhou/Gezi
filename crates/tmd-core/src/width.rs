use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Display width of `text` in terminal columns: the sum of its clusters'
/// widths, so measuring a whole string and adding up its pieces always
/// agree.
///
/// Uses `width()`, never `width_cjk()`: ambiguous characters such as dashes
/// and ellipses take one column, matching WezTerm, kitty and Ghostty.
pub fn text_width(text: &str) -> usize {
    clusters(text).map(cluster_width).sum()
}

/// Display width of one cluster from [`clusters`].
pub fn cluster_width(cluster: &str) -> usize {
    cluster.width()
}

/// Whether `c` takes two columns, as CJK characters, full-width
/// punctuation and most emoji do.
pub fn is_wide(c: char) -> bool {
    c.width() == Some(2)
}

/// Splits `text` into the units a terminal draws as one character: a base
/// character plus any following zero-width marks, variation selectors or
/// emoji skin tones, with zero-width-joiner sequences kept together.
///
/// Every width calculation goes through these units. Summing per-character
/// widths instead disagrees with whole-string widths for sequences like
/// `❤️` (1 vs 2 columns) and splits joined emoji across rows.
pub fn clusters(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let mut chars = rest.char_indices();
        chars.next()?;
        let mut end = rest.len();
        let mut joined = false;
        for (i, c) in chars {
            if joined || continues_cluster(c) {
                joined = c == ZWJ;
                continue;
            }
            end = i;
            break;
        }
        let (cluster, tail) = rest.split_at(end);
        rest = tail;
        Some(cluster)
    })
}

const ZWJ: char = '\u{200D}';

fn continues_cluster(c: char) -> bool {
    c == ZWJ || c.width() == Some(0) || ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
}

/// The longest prefix of `text` that fits in `width` columns, never
/// splitting a cluster.
pub fn truncate(text: &str, width: usize) -> &str {
    let mut used = 0;
    let mut end = 0;
    for cluster in clusters(text) {
        used += cluster_width(cluster);
        if used > width {
            break;
        }
        end += cluster.len();
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clusters_keep_sequences_together() {
        let family = "👨\u{200D}👩\u{200D}👧";
        let text = format!("a\u{2764}\u{FE0F}e\u{301}{family}👍🏽中");
        let parts: Vec<&str> = clusters(&text).collect();
        assert_eq!(
            parts,
            ["a", "\u{2764}\u{FE0F}", "e\u{301}", family, "👍🏽", "中"]
        );
    }

    #[test]
    fn widths_are_additive_over_clusters() {
        assert_eq!(text_width("\u{2764}\u{FE0F}"), 2);
        assert_eq!(text_width("e\u{301}"), 1);
        let text = "中\u{2764}\u{FE0F}ab";
        let parts: usize = clusters(text).map(cluster_width).sum();
        assert_eq!(text_width(text), parts);
    }

    #[test]
    fn truncate_never_splits_a_cluster() {
        assert_eq!(truncate("a\u{2764}\u{FE0F}b", 2), "a");
        assert_eq!(truncate("a\u{2764}\u{FE0F}b", 3), "a\u{2764}\u{FE0F}");
    }
}
