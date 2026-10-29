//! Behavior tests for `render`.

mod cjk;
mod code;
mod html;
mod tables;
mod theme;

use crate::{Color, Line, Style, Theme, render};

fn lines(md: &str, width: u16) -> Vec<Line> {
    render(md, width, &Theme::default())
}

fn plain(md: &str, width: u16) -> Vec<String> {
    lines(md, width).iter().map(Line::plain_text).collect()
}

/// The first span whose text contains `needle`.
fn style_of(lines: &[Line], needle: &str) -> Style {
    lines
        .iter()
        .flat_map(|line| &line.spans)
        .find(|span| span.text.contains(needle))
        .unwrap_or_else(|| panic!("no span contains {needle:?}"))
        .style
        .clone()
}

#[test]
fn headings_hide_markers_and_use_theme_styles() {
    let theme = Theme::default();
    let out = lines("# One\n\n## Two\n\n### Three\n\n###### Six", 20);
    let text: Vec<String> = out.iter().map(Line::plain_text).collect();
    assert_eq!(
        text,
        [
            "One",
            "━".repeat(20).as_str(),
            "",
            "Two",
            "─".repeat(20).as_str(),
            "",
            "Three",
            "",
            "Six"
        ]
    );
    assert_eq!(style_of(&out, "One"), theme.h1);
    assert_eq!(style_of(&out, "Two"), theme.h2);
    assert_eq!(style_of(&out, "Three"), theme.h3);
    assert_eq!(style_of(&out, "Six"), theme.h3);
}

#[test]
fn inline_styles() {
    let out = lines("**b** *i* ~~s~~ `c`", 80);
    assert_eq!(out[0].plain_text(), "b i s c");
    assert!(style_of(&out, "b").bold);
    assert!(style_of(&out, "i").italic);
    assert!(style_of(&out, "s").strike);
    assert_eq!(style_of(&out, "c").bg, Theme::default().inline_code.bg);
}

#[test]
fn nested_inline_styles_combine() {
    let out = lines("**bold *both***", 80);
    let both = style_of(&out, "both");
    assert!(both.bold && both.italic);
}

#[test]
fn links_show_text_underlined_and_keep_the_url() {
    let out = lines("see [docs](https://example.com) and <https://a.io>", 80);
    assert_eq!(out[0].plain_text(), "see docs and https://a.io");
    let docs = style_of(&out, "docs");
    assert!(docs.underline);
    assert_eq!(docs.link.as_deref(), Some("https://example.com"));
    assert_eq!(style_of(&out, "a.io").link.as_deref(), Some("https://a.io"));
    assert_eq!(style_of(&out, "see").link, None);
}

#[test]
fn paragraphs_wrap_to_width() {
    assert_eq!(
        plain("one two three four five", 9),
        ["one two", "three", "four five"]
    );
}

#[test]
fn soft_breaks_join_and_hard_breaks_split() {
    assert_eq!(plain("a\nb", 80), ["a b"]);
    assert_eq!(plain("a  \nb", 80), ["a", "b"]);
}

#[test]
fn blocks_are_separated_by_one_blank_line() {
    assert_eq!(plain("a\n\n\n\nb", 80), ["a", "", "b"]);
}

#[test]
fn wrapped_rows_share_src_and_blank_row_belongs_to_block_above() {
    let md = "aaa bbb\n\nccc";
    let out = lines(md, 3);
    assert_eq!(out.len(), 4);
    let first = out[0].src.clone();
    assert_eq!(&md[first.clone()], "aaa bbb\n");
    assert_eq!(out[1].src, first);
    assert_eq!(out[2].src, first, "blank row belongs to the block above");
    assert_eq!(&md[out[3].src.clone()], "ccc");
}

#[test]
fn unordered_ordered_and_nested_lists() {
    let md = "- a\n- b\n  - c\n    - d\n\n3. x\n4. y";
    assert_eq!(
        plain(md, 80),
        ["• a", "• b", "  ◦ c", "    ▪ d", "", "3. x", "4. y"]
    );
}

#[test]
fn list_item_continuation_rows_are_indented() {
    assert_eq!(plain("- aaa bbb", 6), ["• aaa", "  bbb"]);
}

#[test]
fn loose_lists_keep_blank_rows_between_items() {
    assert_eq!(plain("- a\n\n- b", 80), ["• a", "", "• b"]);
}

#[test]
fn task_lists_use_boxes() {
    assert_eq!(plain("- [ ] todo\n- [x] done", 80), ["☐ todo", "☑ done"]);
}

#[test]
fn quotes_have_bars_and_nest() {
    let md = "> a\n>\n> b\n>> c";
    assert_eq!(plain(md, 80), ["│ a", "│", "│ b", "│", "│ │ c"]);
    let out = lines(md, 80);
    assert_eq!(out[0].spans[0].style, Theme::default().quote_bar);
}

#[test]
fn rule_fills_the_width() {
    assert_eq!(plain("a\n\n---\n\nb", 5), ["a", "", "─────", "", "b"]);
}

#[test]
fn code_block_has_background_and_language_label() {
    let theme = Theme::default();
    let out = lines("```rust\nfn main() {}\n```", 16);
    let text: Vec<String> = out.iter().map(Line::plain_text).collect();
    assert_eq!(text, ["            rust", "fn main() {}    "]);
    for line in &out {
        assert_eq!(line.width(), 16);
        assert!(line.spans.iter().all(|s| s.style.bg == theme.code_block.bg));
    }
    assert!(style_of(&out, "rust").dim);
}

#[test]
fn code_block_without_language_has_no_label_row() {
    assert_eq!(plain("```\nx\n```", 4), ["x   "]);
}

#[test]
fn code_block_keeps_markdown_markers_and_expands_tabs() {
    assert_eq!(
        plain("```\n# **x**\n\ty\n```", 10),
        ["# **x**   ", "    y     "]
    );
}

#[test]
fn long_code_lines_wrap_inside_the_block() {
    assert_eq!(plain("```\nabcdef\n```", 4), ["abcd", "ef  "]);
}

#[test]
fn image_placeholders_are_muted() {
    let theme = Theme::default();
    let out = lines("![a cat](cat.png) and ![](x.png)", 80);
    assert_eq!(out[0].plain_text(), "[图片: a cat] and [图片]");
    assert_eq!(style_of(&out, "图片").fg, theme.muted.fg);
}

#[test]
fn control_characters_are_removed() {
    let out = plain("a\u{1b}[31mb\u{7}c", 80);
    assert_eq!(out, ["a[31mbc"]);
}

#[test]
fn rows_never_exceed_the_width() {
    let md = "# heading that is long\n\n\
              > quote with words\n>\n> - item in quote that wraps\n\n\
              1. first item with text\n   - nested item with more text\n\n\
              ```\nlong code line here\n```\n\n---";
    for width in [8u16, 13, 20, 40] {
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
fn colors_are_ansi_indexes_in_the_default_theme() {
    let theme = Theme::default();
    assert!(matches!(theme.h1.fg, Color::Ansi(n) if n < 16));
}

#[test]
fn deep_nesting_and_long_markers_still_fit_narrow_widths() {
    let cases = [
        "> > > > > > quote text here",
        "- a\n  - b\n    - c\n      - d\n        - e\n          - f\n            - g",
        "123456789. x",
        "- item\n  > quote\n  > - inner\n  >   - inner2\n  >     > deep",
        "> | 表头 | b |\n> |-|-|\n> | 中文内容 | d |",
    ];
    for md in cases {
        for width in 1u16..=20 {
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
fn lists_separated_by_blank_lines_are_loose_without_paragraphs() {
    assert_eq!(plain("- > a\n\n- > b", 80), ["• │ a", "", "• │ b"]);
    assert_eq!(plain("- ### h1\n\n- ### h2", 80), ["• h1", "", "• h2"]);
    assert_eq!(plain("> - a\n>\n> - b", 80), ["│ • a", "│", "│ • b"]);
    assert_eq!(plain("- a\n- b", 80), ["• a", "• b"]);
}

#[test]
fn image_inside_image_alt_keeps_the_outer_label() {
    assert_eq!(plain("![a ![b](x) c](y)", 80), ["[图片: a b c]"]);
}
