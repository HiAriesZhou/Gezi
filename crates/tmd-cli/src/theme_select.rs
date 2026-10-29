//! Picking the theme for `--theme`: user theme files, built-in themes,
//! and `auto`, which matches the terminal's background.

use std::fs;
use std::path::{Path, PathBuf};

use tmd_core::Theme;

/// Theme name that follows the terminal's background.
pub const AUTO: &str = "auto";

/// Whether `auto` should pick a dark theme. Only asks the terminal when
/// stdout is one; then falls back to `COLORFGBG`, then to dark.
pub fn auto_is_dark(
    stdout_is_terminal: bool,
    query: impl FnOnce() -> Option<bool>,
    colorfgbg: Option<&str>,
) -> bool {
    if !stdout_is_terminal {
        return true;
    }
    query()
        .or_else(|| colorfgbg.and_then(colorfgbg_is_dark))
        .unwrap_or(true)
}

/// Reads the background from `COLORFGBG` (`fg;bg` or `fg;other;bg`):
/// ANSI 0–6 and 8 are dark backgrounds, 7 and 9–15 light ones.
fn colorfgbg_is_dark(value: &str) -> Option<bool> {
    let bg: u8 = value.rsplit(';').next()?.trim().parse().ok()?;
    match bg {
        0..=6 | 8 => Some(true),
        7 | 9..=15 => Some(false),
        _ => None,
    }
}

/// The directory holding `tmd/themes`: `$XDG_CONFIG_HOME`, else
/// `~/.config`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

/// Loads `name` from `<config>/tmd/themes/<name>.toml` when that file
/// exists, otherwise from the built-in themes.
pub fn load(name: &str, config_dir: Option<&Path>) -> Result<Theme, String> {
    // The name becomes a file name; keep it from reaching other folders.
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid {
        return Err(format!(
            "invalid theme name {name:?}: use letters, digits, - and _"
        ));
    }
    if let Some(path) = config_dir.map(|dir| dir.join(format!("tmd/themes/{name}.toml")))
        && path.is_file()
    {
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("theme file {}: {err}", path.display()))?;
        return Theme::from_toml(&source)
            .map_err(|err| format!("theme file {}: {err}", path.display()));
    }
    Theme::builtin(name).ok_or_else(|| {
        let names: Vec<&str> = Theme::builtin_names().collect();
        format!(
            "unknown theme {name:?}; built-in themes: {}, or {AUTO}",
            names.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_does_not_query_when_stdout_is_not_a_terminal() {
        let query = || panic!("must not query the terminal");
        assert!(auto_is_dark(false, query, Some("0;15")));
    }

    #[test]
    fn auto_prefers_the_terminal_answer() {
        assert!(!auto_is_dark(true, || Some(false), Some("15;0")));
        assert!(auto_is_dark(true, || Some(true), Some("0;15")));
    }

    #[test]
    fn auto_falls_back_to_colorfgbg_then_dark() {
        assert!(!auto_is_dark(true, || None, Some("0;15")));
        assert!(!auto_is_dark(true, || None, Some("0;7")));
        assert!(auto_is_dark(true, || None, Some("15;0")));
        assert!(auto_is_dark(true, || None, Some("15;default;8")));
        assert!(auto_is_dark(true, || None, Some("default;default")));
        assert!(auto_is_dark(true, || None, Some("garbage")));
        assert!(auto_is_dark(true, || None, None));
    }

    /// A fresh directory under the system temp dir, removed on drop.
    struct TempConfig(PathBuf);

    impl TempConfig {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("tmd-test-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join("tmd/themes")).unwrap();
            Self(dir)
        }

        fn write(&self, name: &str, source: &str) {
            fs::write(self.0.join(format!("tmd/themes/{name}.toml")), source).unwrap();
        }
    }

    impl Drop for TempConfig {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn built_in_themes_load_by_name() {
        assert_eq!(
            load("neon-tokyo", None).unwrap(),
            Theme::builtin("neon-tokyo").unwrap()
        );
        let config = TempConfig::new("empty");
        assert_eq!(
            load("default-light", Some(&config.0)).unwrap(),
            Theme::builtin("default-light").unwrap()
        );
    }

    #[test]
    fn user_theme_overrides_a_built_in_one() {
        let config = TempConfig::new("override");
        config.write("neon-tokyo", "[styles]\nh1 = { fg = \"ansi:1\" }\n");
        let theme = load("neon-tokyo", Some(&config.0)).unwrap();
        assert_eq!(theme.h1.fg, tmd_core::Color::Ansi(1));
    }

    #[test]
    fn user_theme_errors_name_the_file_and_line() {
        let config = TempConfig::new("broken");
        config.write("mine", "dark = true\n[styles]\nh1 = { fg = \"blu\" }\n");
        let err = load("mine", Some(&config.0)).unwrap_err();
        assert!(err.contains("mine.toml"), "{err}");
        assert!(err.contains("line 3"), "{err}");
    }

    #[test]
    fn unknown_theme_lists_the_built_in_ones() {
        let err = load("nope", None).unwrap_err();
        assert!(err.contains("nope") && err.contains("neon-tokyo"), "{err}");
    }

    #[test]
    fn theme_names_cannot_leave_the_themes_directory() {
        for name in ["../secret", "a/b", "", ".hidden", "x y"] {
            assert!(load(name, None).is_err(), "{name:?}");
        }
    }
}
