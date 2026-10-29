//! Display width, CJK wrapping and soft breaks.

use super::{lines, plain};

#[test]
fn display_width_follows_unicode_width() {
    use crate::width::text_width;
    assert_eq!(text_width("中文"), 4);
    assert_eq!(
        text_width("——……"),
        4,
        "ambiguous dashes and ellipses are 1 column"
    );
    assert_eq!(text_width("😀"), 2);
    assert_eq!(text_width("e\u{301}"), 1, "combining marks are 0 columns");
    assert_eq!(text_width("，"), 2);
}

#[test]
fn cjk_wraps_between_any_two_characters() {
    assert_eq!(plain("一二三四五六", 6), ["一二三", "四五六"]);
    assert_eq!(plain("中文abc", 5), ["中文", "abc"]);
    assert_eq!(plain("中文 English 混排", 8), ["中文", "English", "混排"]);
}

#[test]
fn soft_breaks_between_full_width_characters_add_no_space() {
    assert_eq!(plain("中文\n中文", 80), ["中文中文"]);
    assert_eq!(plain("出现。\n第三行", 80), ["出现。第三行"]);
    assert_eq!(plain("中文\nEnglish", 80), ["中文 English"]);
    assert_eq!(plain("English\n中文", 80), ["English 中文"]);
    assert_eq!(plain("a\nb", 80), ["a b"]);
    assert_eq!(plain("**中文**\n中文", 80), ["中文中文"]);
}

#[test]
fn code_blocks_pad_to_display_width() {
    for line in lines("```\n你好 tmd\n```", 12) {
        assert_eq!(line.width(), 12, "{:?}", line.plain_text());
    }
}
