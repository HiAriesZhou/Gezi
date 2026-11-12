use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Config(PathBuf);

impl Config {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "gezi-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn render(&self, name: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_gezi"))
            .args(["--theme", name, "--ansi", "--images", "off"])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../fixtures/basic.md"
            ))
            .env("XDG_CONFIG_HOME", &self.0)
            .output()
            .unwrap()
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn gezi_configuration_takes_precedence_and_legacy_files_still_work() {
    let config = Config::new();
    config.write("tmd/themes/custom.toml", "dark=true\n");
    assert!(config.render("custom").status.success());
    config.write("gezi/themes/custom.toml", "dark='invalid'\n");
    let output = config.render("custom");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("gezi/themes/custom.toml"));
    config.write("gezi/themes/custom.toml", "dark=false\n");
    assert!(config.render("custom").status.success());
}
