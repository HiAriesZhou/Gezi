//! Shared configuration location; individual settings keep their own schemas.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Read a Gezi config file, using the old location only when that file is absent.
/// An unreadable or invalid new file must never silently use the legacy file.
pub fn read(base: &Path, relative: &str) -> Result<Option<(PathBuf, String)>, String> {
    for name in ["gezi", "tmd"] {
        let path = base.join(name).join(relative);
        match fs::read_to_string(&path) {
            Ok(source) => return Ok(Some((path, source))),
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(format!("{}: {err}", path.display())),
        }
    }
    Ok(None)
}

/// The directory holding `gezi/` and legacy `tmd/`: `$XDG_CONFIG_HOME`, else
/// `~/.config`.
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}
