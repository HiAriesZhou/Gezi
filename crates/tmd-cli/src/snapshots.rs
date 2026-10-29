//! Snapshot tests over `fixtures/*.md`: plain text and ANSI output.

use std::fs;
use std::path::PathBuf;

use tmd_core::{Line, Theme, render};

use crate::ansi::to_ansi;

const WIDTH: u16 = 80;
const FIXTURES: [&str; 4] = ["basic", "lists", "quotes", "code"];

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
        let lines = render(&fixture(name), WIDTH, &Theme::default());
        insta::assert_snapshot!(format!("{name}.plain"), plain(&lines));
        // ESC is shown as `\e` so snapshot files stay readable.
        let ansi = to_ansi(&lines, false).replace('\x1b', "\\e");
        insta::assert_snapshot!(format!("{name}.ansi"), ansi);
    }
}
