//! Shared configuration location; individual settings keep their own schemas.
use std::path::PathBuf;

/// The directory holding `tmd/config.toml` and `tmd/themes`: `$XDG_CONFIG_HOME`, else
/// `~/.config`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}
