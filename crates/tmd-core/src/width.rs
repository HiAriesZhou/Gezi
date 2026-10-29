use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Display width of `text` in terminal columns.
///
/// Uses `width()`, never `width_cjk()`: ambiguous characters such as dashes
/// and ellipses take one column, matching WezTerm, kitty and Ghostty.
pub fn text_width(text: &str) -> usize {
    text.width()
}

/// Display width of a single character; control characters count as 0.
pub fn char_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

/// Whether `c` takes two columns, as CJK characters, full-width
/// punctuation and most emoji do.
pub fn is_wide(c: char) -> bool {
    char_width(c) == 2
}

/// The longest prefix of `text` that fits in `width` columns.
pub fn truncate(text: &str, width: usize) -> &str {
    let mut used = 0;
    for (i, c) in text.char_indices() {
        used += char_width(c);
        if used > width {
            return &text[..i];
        }
    }
    text
}
