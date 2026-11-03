//! Optional reader configuration; theme files keep their existing format.
use std::{fs, io, path::Path};

use crate::cli::ReaderMode;

pub fn load(explicit: Option<ReaderMode>, config_dir: Option<&Path>) -> Result<ReaderMode, String> {
    if let Some(mode) = explicit {
        return Ok(mode);
    }
    let Some(path) = config_dir.map(|dir| dir.join("tmd/config.toml")) else {
        return Ok(ReaderMode::Inline);
    };
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(ReaderMode::Inline),
        Err(err) => return Err(format!("reader config {}: {err}", path.display())),
    };
    parse(&source).map_err(|err| format!("reader config {}: {err}", path.display()))
}

fn parse(source: &str) -> Result<ReaderMode, String> {
    let config: toml::Table = toml::from_str(source).map_err(|err| err.to_string())?;
    for key in config.keys() {
        if key != "reader" {
            return Err(format!("unknown setting {key:?}"));
        }
    }
    let Some(reader) = config.get("reader") else {
        return Ok(ReaderMode::Inline);
    };
    let reader = reader.as_table().ok_or("reader must be a table")?;
    if reader.keys().any(|key| key != "mode") {
        return Err("only reader.mode is supported".into());
    }
    match reader.get("mode") {
        None => Ok(ReaderMode::Inline),
        Some(value) => match value.as_str() {
            Some("inline") => Ok(ReaderMode::Inline),
            Some("native") => Ok(ReaderMode::Native),
            _ => Err("reader.mode must be \"inline\" or \"native\"".into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_modes_are_explicit() {
        assert_eq!(parse("").unwrap(), ReaderMode::Inline);
        assert_eq!(
            parse("[reader]\nmode='native'").unwrap(),
            ReaderMode::Native
        );
        assert_eq!(
            load(Some(ReaderMode::Inline), None).unwrap(),
            ReaderMode::Inline
        );
        for source in [
            "reader=1",
            "[reader]\nmode='typo'",
            "[reader]\nmode=1",
            "[reader]\nother=true",
            "[typo]",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
    }
}
