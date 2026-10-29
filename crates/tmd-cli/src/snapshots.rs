//! Snapshot tests over `fixtures/*.md`: plain text at several widths, and
//! ANSI output at one width since styles do not depend on width.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use tmd_core::{Line, Theme, render};

use crate::ansi::to_ansi;

const WIDTHS: [u16; 3] = [40, 80, 120];
const ANSI_WIDTH: u16 = 80;
const FIXTURES: [&str; 8] = [
    "basic",
    "lists",
    "quotes",
    "code",
    "cjk-table",
    "wrap",
    "html",
    "long",
];

fn fixture(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "fixtures"]
        .iter()
        .collect::<PathBuf>()
        .join(format!("{name}.md"));
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn plain(lines: &[Line]) -> String {
    lines.iter().map(|line| line.plain_text() + "\n").collect()
}

#[test]
fn fixtures() {
    for name in FIXTURES {
        let md = fixture(name);
        for width in WIDTHS {
            let lines = render(&md, width, &Theme::default());
            insta::assert_snapshot!(format!("{name}.w{width}"), plain(&lines));
        }
        let lines = render(&md, ANSI_WIDTH, &Theme::default());
        // ESC is shown as `\e` so snapshot files stay readable.
        let ansi = to_ansi(&lines, false).replace('\x1b', "\\e");
        insta::assert_snapshot!(format!("{name}.ansi"), ansi);
    }
}

#[test]
fn cjk_table_rows_have_equal_width() {
    const BORDERS: [char; 4] = ['┌', '│', '├', '└'];
    let md = fixture("cjk-table");
    for width in WIDTHS {
        let mut tables: BTreeMap<usize, Vec<&Line>> = BTreeMap::new();
        let lines = render(&md, width, &Theme::default());
        for line in &lines {
            if line.plain_text().starts_with(BORDERS) {
                tables.entry(line.src.start).or_default().push(line);
            }
        }
        assert_eq!(tables.len(), 4, "width {width}: expected 4 tables");
        for rows in tables.values() {
            let first = rows[0].width();
            assert!(first <= usize::from(width), "width {width}");
            for row in rows {
                assert_eq!(row.width(), first, "width {width}: {:?}", row.plain_text());
            }
        }
    }
}

#[test]
fn html_tags_never_reach_the_output() {
    // Tags the fixtures use; `<tag>` decoded from `&lt;tag&gt;` and a stray
    // `<unclosed` are text and may appear.
    const TAGS: [&str; 13] = [
        "kbd", "b", "span", "img", "details", "summary", "div", "p", "h1", "a", "br", "script",
        "!--",
    ];
    for name in ["basic", "html"] {
        let text = plain(&render(&fixture(name), 80, &Theme::default()));
        for tag in TAGS {
            for marker in [format!("<{tag}>"), format!("<{tag} "), format!("</{tag}>")] {
                assert!(!text.contains(&marker), "{name}: {marker} in output");
            }
        }
        assert!(
            !text.contains("不应该显示") && !text.contains("alert"),
            "{name}"
        );
    }
}
