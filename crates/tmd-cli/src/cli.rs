use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Width used when stdout is not a terminal or its size is unknown.
pub const FALLBACK_WIDTH: u16 = 80;

/// Render markdown in the terminal.
#[derive(Debug, Parser)]
#[command(name = "tmd", version, about)]
pub struct Cli {
    /// Markdown file to render; reads stdin when omitted
    pub file: Option<PathBuf>,

    /// Always output text with ANSI color codes
    #[arg(long)]
    pub ansi: bool,

    /// Experimental double-size H1 in static Markdown files on Kaku
    #[arg(long)]
    pub large_headings: bool,

    /// Static file reader: current-screen scrolling or configured Kaku integration
    #[arg(long, value_enum)]
    pub reader: Option<ReaderMode>,

    /// Theme name
    #[arg(long, value_name = "NAME", default_value = "auto")]
    pub theme: String,

    /// Output width in columns [default: terminal width, or 80 when not a terminal]
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    pub width: Option<u16>,

    /// Input format
    #[arg(long, value_enum, default_value_t = InputFormat::Text)]
    pub from: InputFormat,

    /// Report malformed JSONL records to stderr (without their contents)
    #[arg(long)]
    pub debug: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReaderMode {
    Inline,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InputFormat {
    /// Plain markdown text
    Text,
    /// Claude Code `--output-format stream-json` events
    ClaudeJson,
}

/// Picks the output width: an explicit `--width` wins, then the terminal
/// width when stdout is a terminal, then [`FALLBACK_WIDTH`].
pub fn resolve_width(explicit: Option<u16>, terminal_width: Option<u16>) -> u16 {
    explicit.or(terminal_width).unwrap_or(FALLBACK_WIDTH)
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_all_options() {
        let cli = Cli::try_parse_from([
            "tmd",
            "--ansi",
            "--large-headings",
            "--reader",
            "native",
            "--theme",
            "neon-tokyo",
            "--width",
            "40",
            "--from",
            "claude-json",
            "a.md",
        ])
        .unwrap();
        assert!(cli.ansi);
        assert!(cli.large_headings);
        assert_eq!(cli.reader, Some(ReaderMode::Native));
        assert_eq!(cli.theme, "neon-tokyo");
        assert_eq!(cli.width, Some(40));
        assert_eq!(cli.from, InputFormat::ClaudeJson);
        assert_eq!(cli.file, Some(PathBuf::from("a.md")));
    }

    #[test]
    fn defaults() {
        let cli = Cli::try_parse_from(["tmd"]).unwrap();
        assert!(!cli.ansi);
        assert!(!cli.large_headings);
        assert_eq!(cli.reader, None);
        assert_eq!(cli.theme, "auto");
        assert_eq!(cli.width, None);
        assert_eq!(cli.from, InputFormat::Text);
        assert_eq!(cli.file, None);
    }

    #[test]
    fn rejects_zero_width() {
        assert!(Cli::try_parse_from(["tmd", "--width", "0"]).is_err());
    }

    #[test]
    fn width_prefers_explicit_then_terminal_then_fallback() {
        assert_eq!(resolve_width(Some(40), Some(120)), 40);
        assert_eq!(resolve_width(None, Some(120)), 120);
        assert_eq!(resolve_width(None, None), FALLBACK_WIDTH);
    }
}
