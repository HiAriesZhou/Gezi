use std::io::Write;
use std::process::{Command, Stdio};

const BINARIES: [&str; 2] = [env!("CARGO_BIN_EXE_gezi"), env!("CARGO_BIN_EXE_ge")];

#[test]
fn both_commands_use_the_gezi_brand() {
    for binary in BINARIES {
        let output = Command::new(binary).arg("--version").output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("gezi {}\n", env!("CARGO_PKG_VERSION"))
        );
        let help = Command::new(binary).arg("help").output().unwrap();
        assert!(help.status.success());
        assert!(
            String::from_utf8(help.stdout)
                .unwrap()
                .contains("Usage: gezi")
        );
    }
}

#[test]
fn short_command_renders_the_same_document() {
    let outputs = BINARIES.map(|binary| {
        let mut child = Command::new(binary)
            .args(["--theme", "default-dark", "--width", "40"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all("# 格子\n\n中文 and English.\n".as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains("中文"));
        output.stdout
    });
    assert_eq!(outputs[0], outputs[1]);
}
