//! Code blocks: one source range per code line, so streaming can commit
//! a long block line by line.

use super::{lines, plain};

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
