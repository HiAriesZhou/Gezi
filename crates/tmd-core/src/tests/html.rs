//! HTML shown as text: tags hidden, breaks and images kept.

use super::{lines, plain, style_of};
use crate::Theme;

#[test]
fn inline_tags_are_hidden_and_text_keeps_its_style() {
    let theme = Theme::default();
    let out = lines("按 <kbd>Ctrl</kbd>+<kbd>C</kbd>，**<b>粗</b>**", 80);
    assert_eq!(out[0].plain_text(), "按 Ctrl+C，粗");
    assert_eq!(style_of(&out, "Ctrl"), theme.text);
    assert!(style_of(&out, "粗").bold);
}

#[test]
fn inline_br_breaks_and_img_becomes_a_placeholder() {
    let theme = Theme::default();
    assert_eq!(plain("a<br>b", 80), ["a", "b"]);
    let out = lines(r#"x <img src="a.png" alt="猫"> y"#, 80);
    assert_eq!(out[0].plain_text(), "x [图片: 猫] y");
    assert_eq!(style_of(&out, "图片").fg, theme.muted.fg);
}

#[test]
fn inline_comments_are_hidden() {
    assert_eq!(plain("a<!-- hidden -->b", 80), ["ab"]);
}

#[test]
fn html_block_shows_its_text_like_a_paragraph() {
    let md = "<details>\n<summary>更多</summary>\n内容 &amp; 细节\n</details>\n\n下一段";
    assert_eq!(plain(md, 80), ["更多", "内容 & 细节", "", "下一段"]);
    assert_eq!(style_of(&lines(md, 80), "内容"), Theme::default().text);
}

#[test]
fn readme_style_header() {
    let md = r#"<p align="center">
  <img src="logo.png" alt="TMD logo" width="120">
</p>
<p align="center">
  <a href="x"><img src="ci.svg" alt="CI"></a>
  <a href="y"><img src="lic.svg" alt="License: MIT"></a>
</p>

# tmd"#;
    assert_eq!(
        plain(md, 80),
        [
            "[图片: TMD logo]",
            "[图片: CI] [图片: License: MIT]",
            "",
            "tmd"
        ]
    );
}

#[test]
fn html_block_with_only_tags_or_comments_leaves_no_row() {
    assert_eq!(plain("a\n\n<!-- note -->\n\nb", 80), ["a", "", "b"]);
    assert_eq!(plain("a\n\n<div></div>\n\nb", 80), ["a", "", "b"]);
}

#[test]
fn html_block_text_wraps_to_width() {
    assert_eq!(plain("<div>one two three</div>", 9), ["one two", "three"]);
}

#[test]
fn inline_script_content_is_hidden() {
    assert_eq!(
        plain("text <script>alert(1)</script> after", 80),
        ["text  after"]
    );
    assert_eq!(plain("a <style>p { color: red }</style>b", 80), ["a b"]);
}

#[test]
fn html_table_cells_are_separated() {
    assert_eq!(
        plain("<table><tr><td>a</td><td>b</td></tr></table>", 80),
        ["a b"]
    );
}
