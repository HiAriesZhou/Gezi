//! Tables.

use super::{lines, plain, style_of};
use crate::Theme;

#[test]
fn table_with_borders_and_alignment() {
    let md = "| a | bb | c |\n|:-|--:|:-:|\n| ccc | d | e |";
    assert_eq!(
        plain(md, 80),
        [
            "┌─────┬────┬───┐",
            "│ a   │ bb │ c │",
            "├─────┼────┼───┤",
            "│ ccc │  d │ e │",
            "└─────┴────┴───┘",
        ]
    );
}

#[test]
fn table_center_alignment_splits_padding() {
    let md = "| abcd |\n|:-:|\n| a |\n| ab |";
    assert_eq!(
        plain(md, 80),
        [
            "┌──────┐",
            "│ abcd │",
            "├──────┤",
            "│  a   │",
            "│  ab  │",
            "└──────┘",
        ]
    );
}

#[test]
fn table_header_is_bold_and_borders_use_theme() {
    let theme = Theme::default();
    let out = lines("| head |\n|-|\n| body |", 80);
    assert!(style_of(&out, "head").bold);
    assert!(!style_of(&out, "body").bold);
    assert_eq!(style_of(&out, "┌"), theme.table_border);
}

#[test]
fn wide_table_shrinks_the_widest_column_and_wraps_cells() {
    let md = "| aaaaaaaaaa | bb |\n|-|-|\n| x | y |";
    assert_eq!(
        plain(md, 14),
        [
            "┌───────┬────┐",
            "│ aaaaa │ bb │",
            "│ aaaaa │    │",
            "├───────┼────┤",
            "│ x     │ y  │",
            "└───────┴────┘",
        ]
    );
}

#[test]
fn table_too_wide_after_shrinking_is_cut_with_ellipsis() {
    let md = "| aaaa | bbbb | cccc |\n|-|-|-|\n| 中文 | x | y |";
    for line in lines(md, 10) {
        assert_eq!(line.width(), 10, "{:?}", line.plain_text());
        assert!(line.plain_text().ends_with('…'));
    }
}

#[test]
fn cjk_table_rows_have_equal_width() {
    let md = "| 名称 | Name | 说明 |\n|:--|:-:|--:|\n| 中文 | English | 全角，标点 |\n| 😀 | ——…… | 混排 mixed |";
    for width in [20u16, 40, 80] {
        let out = lines(md, width);
        let first = out[0].width();
        assert!(first <= usize::from(width));
        for line in &out {
            assert_eq!(
                line.width(),
                first,
                "width {width}: {:?}",
                line.plain_text()
            );
        }
    }
}
