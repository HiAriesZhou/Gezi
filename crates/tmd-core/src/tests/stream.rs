//! Streaming: chunks pushed in any split commit exactly the lines a full
//! render produces, each once.

use proptest::prelude::*;

use crate::{Line, Streamer, Theme, render};

const FIXTURES: [(&str, &str); 7] = [
    ("basic", include_str!("../../../../fixtures/basic.md")),
    ("lists", include_str!("../../../../fixtures/lists.md")),
    ("quotes", include_str!("../../../../fixtures/quotes.md")),
    ("code", include_str!("../../../../fixtures/code.md")),
    (
        "cjk-table",
        include_str!("../../../../fixtures/cjk-table.md"),
    ),
    ("wrap", include_str!("../../../../fixtures/wrap.md")),
    ("html", include_str!("../../../../fixtures/html.md")),
];

/// Splits `md` into chunks of the given byte sizes (cycled), moved back to
/// character boundaries.
fn chunks<'a>(md: &'a str, sizes: &[usize]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while start < md.len() {
        let mut end = (start + sizes[i % sizes.len()]).min(md.len());
        while !md.is_char_boundary(end) {
            end += 1;
        }
        out.push(&md[start..end]);
        start = end;
        i += 1;
    }
    out
}

/// Everything `Streamer` commits for `pieces`, then the final frame.
fn stream(pieces: &[&str], width: u16) -> (Vec<Line>, Vec<Vec<Line>>) {
    let mut streamer = Streamer::new(width, Theme::default());
    let mut all = Vec::new();
    let mut per_push = Vec::new();
    for piece in pieces {
        let frame = streamer.push(piece);
        per_push.push(frame.committed.clone());
        all.extend(frame.committed);
    }
    let last = streamer.finish();
    assert!(last.active.is_empty(), "finish leaves nothing active");
    all.extend(last.committed);
    (all, per_push)
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(Line::plain_text).collect()
}

#[test]
fn code_blocks_commit_line_by_line() {
    let mut s = Streamer::new(20, Theme::default());
    let frame = s.push("```rust\nfn a()");
    assert!(frame.committed.is_empty(), "no complete code line yet");
    assert!(!frame.active.is_empty());
    let frame = s.push(" {}\nlet");
    assert_eq!(
        texts(&frame.committed).len(),
        2,
        "label row and the first line"
    );
    assert!(frame.committed[1].plain_text().starts_with("fn a() {}"));
    assert!(texts(&frame.active)[0].starts_with("let"));
}

#[test]
fn label_and_empty_block_rows_wait_for_a_code_line() {
    // Split points where a model often stops: right after the fence line.
    for (md, at) in [
        ("```rust\nfn x\n```\n", 8),
        ("```\n\nx\n```\n", 4),
        ("```rust\n", 8),
    ] {
        let (all, _) = stream(&[&md[..at], &md[at..]], 30);
        assert_eq!(
            texts(&all),
            texts(&render(md, 30, &Theme::default())),
            "{md:?}"
        );
    }
}

#[test]
fn blank_row_after_a_code_block_is_not_lost() {
    let md = "```rust\na\nb\n```\n\npara\n";
    let (all, _) = stream(&[&md[..10], &md[10..]], 30);
    assert_eq!(all, render(md, 30, &Theme::default()));
}

#[test]
fn the_last_block_stays_active_until_another_starts() {
    let mut s = Streamer::new(40, Theme::default());
    let frame = s.push("| a | b |\n|---|---|\n| 1 | 2 |\n");
    assert!(frame.committed.is_empty(), "a table is the last block");
    let frame = s.push("\nafter\n");
    assert_eq!(
        texts(&frame.committed).first().map(String::as_str),
        Some("┌───┬───┐")
    );
    assert_eq!(
        texts(&frame.active),
        ["", "after"],
        "the blank row goes with the block below"
    );
}

/// Rows of one block share a source range and are committed together; a
/// range committed by one push must never come back in a later one.
#[test]
fn committed_lines_never_repeat() {
    for (name, md) in FIXTURES {
        let pieces: Vec<&str> = md.split_inclusive('\n').collect();
        let (_, per_push) = stream(&pieces, 80);
        let mut earlier = std::collections::HashSet::new();
        for lines in per_push {
            let this_push: std::collections::HashSet<_> =
                lines.iter().map(|line| line.src.clone()).collect();
            for src in &this_push {
                assert!(!earlier.contains(src), "{name}: {src:?} committed again");
            }
            earlier.extend(this_push);
        }
    }
}

/// The first row where streamed and full output differ, for messages.
fn first_difference(streamed: &[String], full: &[String]) -> String {
    let i = streamed
        .iter()
        .zip(full)
        .take_while(|(a, b)| a == b)
        .count();
    let around = |v: &[String]| v[i.saturating_sub(2)..(i + 3).min(v.len())].to_vec();
    format!(
        "row {i}: streamed {:?} / full {:?}",
        around(streamed),
        around(full)
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// The core streaming guarantee: any split, same lines as one render.
    #[test]
    fn streaming_matches_a_full_render(
        sizes in prop::collection::vec(1usize..=50, 1..64),
        width in prop::sample::select(vec![24u16, 40, 80, 120]),
    ) {
        for (name, md) in FIXTURES {
            let (all, _) = stream(&chunks(md, &sizes), width);
            let full = render(md, width, &Theme::default());
            let (a, b) = (texts(&all), texts(&full));
            prop_assert!(a == b, "{} at {}: {}", name, width, first_difference(&a, &b));
            prop_assert_eq!(all, full, "{} at {}: styles or sources differ", name, width);
        }
    }
}
