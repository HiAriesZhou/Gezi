use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct Installation {
    root: PathBuf,
    current: Vec<u8>,
    next: String,
}

impl Installation {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "gezi-update-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["installed", "commands", "assets", "package"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        let current = fs::read(env!("CARGO_BIN_EXE_gezi")).unwrap();
        for name in ["gezi", "ge"] {
            write_executable(&root.join("installed").join(name), &current);
        }
        write_executable(
            &root.join("commands/curl"),
            br##"#!/bin/sh
set -eu
out=""
while [ "$#" -gt 0 ]; do
    case "$1" in --output) out="$2"; shift;; esac
    url="$1"
    shift
done
printf '%s\n' "$url" >> "$GEZI_TEST_REQUESTS"
[ "${GEZI_TEST_FAIL:-}" != 1 ] || exit 22
case "$url" in
    */latest) printf '%s' "$GEZI_TEST_LATEST";;
    *) cp "$GEZI_TEST_ASSETS/${url##*/}" "$out";;
esac
"##,
        );
        let mut version: Vec<u64> = env!("CARGO_PKG_VERSION")
            .split('.')
            .map(|part| part.parse().unwrap())
            .collect();
        version[2] += 1;
        let next = version
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(".");
        Self {
            root,
            current,
            next,
        }
    }

    fn package(&self, valid_checksum: bool) {
        let name = format!("gezi-v{}-aarch64-apple-darwin", self.next);
        let directory = self.root.join("package").join(&name);
        fs::create_dir(&directory).unwrap();
        for binary in ["gezi", "ge"] {
            write_executable(
                &directory.join(binary),
                format!("#!/bin/sh\necho 'gezi {}'\n", self.next).as_bytes(),
            );
        }
        let archive = self.root.join("assets").join(format!("{name}.tar.gz"));
        assert!(
            Command::new("/usr/bin/tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(self.root.join("package"))
                .arg(&name)
                .status()
                .unwrap()
                .success()
        );
        let checksum = Command::new("/usr/bin/shasum")
            .args(["-a", "256"])
            .arg(&archive)
            .output()
            .unwrap();
        assert!(checksum.status.success());
        let digest = if valid_checksum {
            String::from_utf8(checksum.stdout)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .to_owned()
        } else {
            "0".repeat(64)
        };
        fs::write(
            archive.with_extension("gz.sha256"),
            format!("{digest}  {name}.tar.gz\n"),
        )
        .unwrap();
    }

    fn command(&self, binary: &str, latest: &str) -> Command {
        let mut command = Command::new(self.root.join("installed").join(binary));
        command
            .arg("update")
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("commands").display()),
            )
            .env("GEZI_TEST_REQUESTS", self.root.join("requests"))
            .env("GEZI_TEST_ASSETS", self.root.join("assets"))
            .env(
                "GEZI_TEST_LATEST",
                format!("https://github.com/HiAriesZhou/Gezi/releases/tag/v{latest}"),
            )
            .env("GEZI_INSTALL_DIR", self.root.join("must-not-install-here"))
            .env("GEZI_VERSION", "v0.0.1")
            .env("TMD_VERSION", "v0.0.1")
            .env("TMD_INSTALL_DIR", self.root.join("must-not-install-here"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn unchanged(&self) {
        for name in ["gezi", "ge"] {
            assert_eq!(
                fs::read(self.root.join("installed").join(name)).unwrap(),
                self.current
            );
        }
        assert!(!self.root.join("must-not-install-here").exists());
    }
}

impl Drop for Installation {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn write_executable(path: &Path, content: &[u8]) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn run(mut command: Command) -> io::Result<Output> {
    let mut child = command.spawn()?;
    let input = child.stdin.take();
    let deadline = Instant::now() + Duration::from_secs(20);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            panic!("update waited for stdin or hung");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(input);
    child.wait_with_output()
}

#[test]
fn both_commands_update_their_own_directory_without_reading_stdin() {
    for name in ["gezi", "ge"] {
        let installation = Installation::new();
        installation.package(true);
        let output = run(installation.command(name, &installation.next)).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = format!("#!/bin/sh\necho 'gezi {}'\n", installation.next);
        for binary in ["gezi", "ge"] {
            assert_eq!(
                fs::read(installation.root.join("installed").join(binary)).unwrap(),
                expected.as_bytes()
            );
        }
        let requests = fs::read_to_string(installation.root.join("requests")).unwrap();
        assert_eq!(requests.lines().count(), 3);
        assert!(
            requests
                .lines()
                .skip(1)
                .all(|url| url.contains(&format!("/download/v{}/", installation.next)))
        );
        assert!(!installation.root.join("must-not-install-here").exists());
    }
}

#[test]
fn equal_or_older_release_does_not_replace_or_download_assets() {
    for latest in [env!("CARGO_PKG_VERSION"), "0.0.1"] {
        let installation = Installation::new();
        let output = run(installation.command("ge", latest)).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("No update needed"));
        installation.unchanged();
        assert_eq!(
            fs::read_to_string(installation.root.join("requests"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }
}

#[test]
fn network_and_install_failures_reach_the_calling_shell() {
    for network_failure in [true, false] {
        let installation = Installation::new();
        installation.package(false);
        let mut command = installation.command("gezi", &installation.next);
        if network_failure {
            command.env("GEZI_TEST_FAIL", "1");
        }
        let output = run(command).unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains(if network_failure {
                "check for updates"
            } else {
                "Checksum mismatch"
            }),
            "{error}"
        );
        installation.unchanged();
    }
}

#[test]
fn update_help_is_local_and_does_not_contact_github() {
    let installation = Installation::new();
    let mut command = installation.command("ge", &installation.next);
    command.arg("--help");
    let output = run(command).unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("update"));
    assert!(!installation.root.join("requests").exists());
    installation.unchanged();
}
