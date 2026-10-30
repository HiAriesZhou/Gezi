use std::cell::Cell;
use std::rc::Rc;

use serde_json::json;

use super::*;

fn source() -> ClaudeStream {
    ClaudeStream::with_clock(80, Theme::default(), false, || Duration::ZERO)
}

fn plain(lines: &[Line]) -> String {
    lines.iter().map(|line| line.plain_text() + "\n").collect()
}

fn send(source: &mut ClaudeStream, event: Value) -> Frame {
    source.push(&(event.to_string() + "\n"))
}

fn stream(event: Value) -> Value {
    json!({"type":"stream_event", "event":event})
}

fn start(index: u64, content: Value) -> Value {
    stream(json!({"type":"content_block_start", "index":index, "content_block":content}))
}

fn delta(index: u64, delta: Value) -> Value {
    stream(json!({"type":"content_block_delta", "index":index, "delta":delta}))
}

fn stop(index: u64) -> Value {
    stream(json!({"type":"content_block_stop", "index":index}))
}

fn replay(input: &str, chunks: bool) -> Vec<Line> {
    let mut source = source();
    let mut lines = Vec::new();
    if chunks {
        // One Unicode character per push splits JSON records, escapes and keys.
        for c in input.chars() {
            lines.extend(source.push(&c.to_string()).committed);
        }
    } else {
        lines.extend(source.push(input).committed);
    }
    let final_frame = source.finish();
    assert!(final_frame.active.is_empty());
    lines.extend(final_frame.committed);
    assert!(source.finish().committed.is_empty());
    lines
}

#[test]
fn real_fixtures_replay_identically_across_chunks() {
    for (name, input) in [
        (
            "plain",
            include_str!("../../../../fixtures/claude/plain.jsonl"),
        ),
        (
            "tools",
            include_str!("../../../../fixtures/claude/tools.jsonl"),
        ),
    ] {
        let expected = replay(input, false);
        assert!(!expected.is_empty());
        assert_eq!(
            replay(input.trim_end_matches('\n'), true),
            expected,
            "{name}"
        );
        let text = plain(&expected);
        assert!(text.contains("▸ 思考 · 内容不可见"));
        assert_eq!(text.matches("完成 · 总用时").count(), 1);
        if name == "plain" {
            assert_eq!(text.matches("▸ 回答中断，重新生成").count(), 1);
            assert!(text.contains("所有权"));
        } else {
            assert_eq!(text.matches("▸ 工具 · Read").count(), 1);
            assert_eq!(text.matches("✓ Read · 用时 0.0 s").count(), 1);
        }
        insta::assert_snapshot!(format!("claude-{name}.w80"), text);
    }
}

#[test]
fn text_is_active_until_stopped_and_assistant_duplicates_are_ignored() {
    let mut s = source();
    send(&mut s, start(0, json!({"type":"text", "text":"你好"})));
    let f = send(
        &mut s,
        delta(0, json!({"type":"text_delta", "text":" **Rust**"})),
    );
    assert!(f.committed.is_empty());
    assert_eq!(plain(&f.active), "你好 Rust\n");
    let f = send(
        &mut s,
        json!({"type":"assistant", "message":{"content":[{"type":"text", "text":"你好 Rust"}]}}),
    );
    assert!(f.committed.is_empty());
    assert_eq!(plain(&f.active), "你好 Rust\n");
    let f = send(&mut s, stop(0));
    assert_eq!(plain(&f.committed), "你好 Rust\n");
    assert!(f.active.is_empty());
    assert!(s.finish().committed.is_empty());
}

#[test]
fn bad_unknown_and_child_events_do_not_corrupt_parent() {
    let mut s = source();
    send(&mut s, start(0, json!({"type":"text"})));
    for line in [
        "broken\n",
        "null\n",
        "[]\n",
        "42\n",
        "{\"type\":\"future\"}\n",
    ] {
        assert!(s.push(line).committed.is_empty());
    }
    let mut child = start(0, json!({"type":"text", "text":"child"}));
    child["parent_tool_use_id"] = json!("parent-tool");
    send(&mut s, child);
    send(
        &mut s,
        delta(9, json!({"type":"text_delta", "text":"wrong-index"})),
    );
    send(&mut s, stop(9));
    send(
        &mut s,
        delta(0, json!({"type":"text_delta", "text":"parent"})),
    );
    let mut stopped = stream(json!({"type":"message_stop"}));
    stopped["abandoned_blocks"] = json!({"from_block_index":0});
    assert_eq!(
        plain(&send(&mut s, stopped).committed),
        "parent\n▸ 回答中断，重新生成\n"
    );
    send(&mut s, start(0, json!({"type":"text", "text":"new"})));
    assert_eq!(plain(&s.finish().committed), "new\n");
}

#[test]
fn thinking_counts_only_visible_characters() {
    let mut s = source();
    send(
        &mut s,
        start(0, json!({"type":"thinking", "thinking":"中"})),
    );
    send(
        &mut s,
        delta(0, json!({"type":"thinking_delta", "thinking":"文a"})),
    );
    assert_eq!(plain(&send(&mut s, stop(0)).committed), "▸ 思考 · 3 字\n");
    send(&mut s, start(1, json!({"type":"thinking"})));
    assert_eq!(plain(&s.finish().committed), "▸ 思考 · 内容不可见\n");
}

#[test]
fn tool_results_pair_by_id_with_injected_clock_and_failures() {
    let clock = Rc::new(Cell::new(Duration::ZERO));
    let copy = clock.clone();
    let mut s = ClaudeStream::with_clock(80, Theme::default(), false, move || copy.get());
    for (id, name) in [("a", "Read"), ("b", "Bash")] {
        send(
            &mut s,
            start(0, json!({"type":"tool_use", "id":id, "name":name})),
        );
        send(
            &mut s,
            delta(
                0,
                json!({"type":"input_json_delta", "partial_json":"{\"file_path\":"}),
            ),
        );
        send(
            &mut s,
            delta(
                0,
                json!({"type":"input_json_delta", "partial_json":"\"README.md\"}"}),
            ),
        );
        assert_eq!(
            plain(&send(&mut s, stop(0)).committed),
            format!("▸ 工具 · {name} · {{\"file_path\":\"README.md\"}}\n")
        );
    }
    clock.set(Duration::from_millis(8400));
    let f = send(
        &mut s,
        json!({"type":"user", "message":{"content":[
            {"type":"tool_result", "tool_use_id":"unknown"},
            {"type":"tool_result", "tool_use_id":"b", "is_error":true},
            {"type":"tool_result", "tool_use_id":"a"}
        ]}}),
    );
    assert_eq!(plain(&f.committed), "✗ Bash · 出错\n✓ Read · 用时 8.4 s\n");
    assert!(send(&mut s, json!({"type":"user", "message":{"content":[{"type":"tool_result", "tool_use_id":"a"}]}})).committed.is_empty());
}

#[test]
fn eof_and_result_flush_active_text_and_only_show_valid_fields() {
    let input = [
        start(0, json!({"type":"text", "text":"尾行"})),
        json!({"type":"result", "duration_ms":1250, "total_cost_usd":0.01234}),
    ]
    .iter()
    .map(Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    assert_eq!(
        plain(&replay(&input, true)),
        "尾行\n完成 · 总用时 1.2 s · $0.0123\n"
    );
    assert_eq!(
        plain(&replay(
            "{\"type\":\"result\",\"duration_ms\":-2,\"total_cost_usd\":\"private\"}",
            false
        )),
        "完成\n"
    );
    let mut s = source();
    send(&mut s, start(0, json!({"type":"text", "text":"EOF"})));
    s.push("{bad final JSON");
    assert_eq!(plain(&s.finish().committed), "EOF\n");
}

#[test]
fn resizing_changes_only_active_text_and_folded_lines_fit() {
    let mut s = source();
    send(
        &mut s,
        start(0, json!({"type":"text", "text":"第一段\n\nabcdefghijk"})),
    );
    s.set_width(5);
    let f = s.push("");
    assert!(f.active.len() > 1);
    assert!(f.active.iter().all(|l| l.width() <= 5));
    send(&mut s, stop(0));
    for width in 1..=80 {
        s.set_width(width);
        let f = send(&mut s, json!({"type":"result", "duration_ms":9999}));
        assert!(f.committed.iter().all(|l| l.width() <= usize::from(width)));
    }
    s.set_width(80);
    send(
        &mut s,
        start(
            0,
            json!({"type":"tool_use", "id":"a", "name":"Bad\u{1b}]52;secret\u{7}\nName", "input":{"x":"👨‍👩‍👧".repeat(100)}}),
        ),
    );
    let text = plain(&send(&mut s, stop(0)).committed);
    assert!(!text.contains('\x1b') && !text.contains('\x07'));
    assert_eq!(text.lines().count(), 1);
}

#[test]
fn upstream_error_is_not_reported_as_success() {
    assert_eq!(
        plain(&replay(r#"{"type":"result","is_error":true}"#, false)),
        "失败\n"
    );
}
