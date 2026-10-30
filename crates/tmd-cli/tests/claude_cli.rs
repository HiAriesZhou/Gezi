use std::io::Write;
use std::process::{Command, Stdio};

fn run(input: &[u8], debug: bool, ansi: bool) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tmd"));
    command.args(["--from", "claude-json", "--theme", "default-dark"]);
    if debug {
        command.arg("--debug");
    }
    if ansi {
        command.arg("--ansi");
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn redirected_and_ansi_modes_interpret_events_and_debug_is_private() {
    let input = b"private-malformed-record\n{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"hello Rust\"}}}\n{\"type\":\"result\"}";
    for ansi in [false, true] {
        for debug in [false, true] {
            let result = run(input, debug, ansi);
            assert!(result.status.success());
            let output = String::from_utf8(result.stdout).unwrap();
            assert!(output.contains("hello Rust"));
            assert!(output.contains("完成"));
            assert!(!output.contains("content_block_start"));
            let errors = String::from_utf8(result.stderr).unwrap();
            assert_eq!(errors.contains("JSONL line 1: invalid JSON"), debug);
            assert!(!errors.contains("private-malformed-record"));
        }
    }
}
