use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn help_command_and_flags_exit_without_waiting_for_stdin() {
    let mut expected = None;
    for argument in ["help", "--help", "-h"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tmd"))
            .arg(argument)
            .env("NO_COLOR", "1")
            .env_remove("CLICOLOR_FORCE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Keep the pipe open: help must never enter the input/rendering path.
        let input = child.stdin.take().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{argument} waited for stdin");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        drop(input);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Usage: tmd"));
        assert!(text.contains("--remote-images"));
        assert!(!text.contains('\x1b'));
        if let Some(expected) = &expected {
            if argument == "--help" {
                assert_eq!(text.trim(), expected);
            }
        } else {
            expected = Some(text.trim().to_owned());
        }
    }
}
