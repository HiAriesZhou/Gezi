//! Code blocks: one source range per code line, so streaming can commit
//! a long block line by line.

use super::{lines, plain, style_of};
use crate::{Color, Theme, render};

#[test]
fn each_code_row_points_at_its_source_line() {
    let md = "```rust\nab\ncd\n```\n";
    let out = lines(md, 10);
    assert_eq!(plain(md, 10), ["      rust", "ab        ", "cd        "]);
    assert_eq!(
        &md[out[0].src.clone()],
        "```rust\n",
        "label row is the opening fence"
    );
    assert_eq!(&md[out[1].src.clone()], "ab\n");
    assert_eq!(&md[out[2].src.clone()], "cd\n");
}

#[test]
fn wrapped_code_rows_share_their_line_source() {
    let md = "```\nabcdef\nx\n```";
    let out = lines(md, 4);
    assert_eq!(plain(md, 4), ["abcd", "ef  ", "x   "]);
    assert_eq!(&md[out[0].src.clone()], "abcdef\n");
    assert_eq!(out[1].src, out[0].src);
    assert_eq!(&md[out[2].src.clone()], "x\n");
}

#[test]
fn unclosed_code_block_lines_still_have_sources() {
    let md = "```\nfirst\nsecond";
    let out = lines(md, 10);
    assert_eq!(&md[out[0].src.clone()], "first\n");
    assert_eq!(&md[out[1].src.clone()], "second");
}

#[test]
fn blank_row_after_a_code_block_belongs_to_the_whole_block() {
    let md = "```\nx\n```\n\nnext";
    let out = lines(md, 10);
    assert_eq!(plain(md, 10)[1], "");
    // pulldown-cmark ends the block range before the closing fence's newline.
    assert_eq!(&md[out[1].src.clone()], "```\nx\n```");
}

#[test]
fn nested_code_rows_use_the_top_level_block_source() {
    let md = "> ```\n> a\n> b\n> ```\n";
    let out = lines(md, 20);
    assert!(out.iter().all(|line| line.src == out[0].src));
    assert_eq!(out[0].src.start, 0);
}

fn has_rgb(lines: &[crate::Line]) -> bool {
    lines
        .iter()
        .flat_map(|line| &line.spans)
        .any(|span| matches!(span.style.fg, Color::Rgb(..)))
}

#[test]
fn known_languages_are_highlighted_on_the_block_background() {
    let theme = Theme::default();
    for md in [
        "```rust\nfn main() {}\n```",
        "```python\ndef f(): pass\n```",
        "```bash\necho \"$HOME\"\n```",
        "```typescript\nconst x: number = 1;\n```",
    ] {
        let out = lines(md, 40);
        assert!(has_rgb(&out), "{md:?} has no highlight colors");
        for line in &out {
            assert_eq!(line.width(), 40);
            assert!(line.spans.iter().all(|s| s.style.bg == theme.code_block.bg));
        }
    }
}

#[test]
fn unknown_or_missing_language_is_not_highlighted() {
    assert!(!has_rgb(&lines("```nosuchlang\nfn main() {}\n```", 40)));
    assert!(!has_rgb(&lines("```\nfn main() {}\n```", 40)));
    assert!(!has_rgb(&lines("```text\nplain words\n```", 40)));
    assert!(!has_rgb(&lines("```txt\nplain words\n```", 40)));
}

#[test]
fn highlighting_carries_state_across_lines() {
    let out = lines("```rust\n/* open\nstill comment */\nfn f() {}\n```", 40);
    let fg = |needle: &str| style_of(&out, needle).fg;
    assert_eq!(fg("still"), fg("open"), "second line of a block comment");
    assert_ne!(fg("fn"), fg("open"));
}

#[test]
fn light_themes_use_light_highlight_colors() {
    let md = "```rust\nfn main() {}\n```";
    let dark = render(md, 40, &Theme::default());
    let light = render(
        md,
        40,
        &Theme {
            dark: false,
            ..Theme::default()
        },
    );
    let fg = |lines: &[crate::Line]| style_of(lines, "fn").fg;
    assert_ne!(fg(&dark), fg(&light));
}

#[test]
fn highlighted_cjk_lines_fill_the_row_exactly() {
    for line in lines("```python\nprint(\"你好，世界\")  # 注释\n```", 20) {
        assert_eq!(line.width(), 20, "{:?}", line.plain_text());
    }
}
