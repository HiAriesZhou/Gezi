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

#[test]
fn closing_punctuation_never_starts_a_row() {
    assert_eq!(plain("一二三，四五", 6), ["一二", "三，四", "五"]);
    assert_eq!(plain("一二——三", 5), ["一", "二——", "三"]);
    assert_eq!(plain("一二三。", 6), ["一二", "三。"]);
}

#[test]
fn opening_punctuation_never_ends_a_row() {
    assert_eq!(plain("一二（三）", 6), ["一二", "（三）"]);
    assert_eq!(plain("一二「三", 6), ["一二", "「三"]);
}

#[test]
fn rows_still_fit_with_punctuation_rules() {
    let md = "「这是引号」，（这是括号），《这是书名号》——破折号……省略号。";
    for width in 4u16..=30 {
        for line in lines(md, width) {
            assert!(
                line.width() <= usize::from(width),
                "width {width}: {:?}",
                line.plain_text()
            );
        }
    }
}

#[test]
fn emoji_sequences_never_panic_or_overflow() {
    let heart = "\u{2764}\u{FE0F}";
    let row = heart.repeat(5);
    let cases = [
        row.clone(),
        format!("```\n{row}\n```"),
        format!("```{row}\nx\n```"),
        format!("| a | b |\n|---|---|\n| {} | {row} |", heart.repeat(6)),
    ];
    for md in &cases {
        for width in 1u16..=40 {
            for line in lines(md, width) {
                assert!(
                    line.width() <= usize::from(width),
                    "width {width}, {md:?}: {:?}",
                    line.plain_text()
                );
            }
        }
    }
}

#[test]
fn emoji_zwj_sequences_stay_whole() {
    let family = "👨\u{200D}👩\u{200D}👧";
    assert_eq!(plain(&family.repeat(2), 2), [family, family]);
}
