/// Display width of `text` in terminal columns.
///
/// Counts characters for now; T3 switches this to `unicode-width`.
pub fn text_width(text: &str) -> usize {
    text.chars().count()
}

/// Display width of a single character.
pub fn char_width(c: char) -> usize {
    let _ = c;
    1
}
