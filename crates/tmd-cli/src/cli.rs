use std::path::PathBuf;

use crate::i18n::{self, LanguageChoice};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};

/// Width used when stdout is not a terminal or its size is unknown.
pub const FALLBACK_WIDTH: u16 = 80;

/// Render markdown in the terminal.
#[derive(Debug, Parser)]
#[command(
    name = "gezi",
    version,
    about,
    disable_help_subcommand = true,
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    /// Interface language: auto follows Kaku, then the system, then English
    #[arg(long, global = true, value_enum)]
    pub language: Option<LanguageChoice>,

    #[command(subcommand)]
    pub command: Option<Command>,

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

    /// Inline reader images on supported Kaku terminals
    #[arg(long, value_enum, default_value_t = ImageMode::Auto)]
    pub images: ImageMode,

    /// Load HTTP(S) images in the inline reader
    #[arg(long, value_enum, default_value_t = RemoteImages::On)]
    pub remote_images: RemoteImages,

    /// Theme name
    #[arg(long, value_name = "NAME", default_value = "auto")]
    pub theme: String,

    /// Content width in columns [default: up to 100 in the interactive reader;
    /// otherwise terminal width, or 80 when not a terminal]
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    pub width: Option<u16>,

    /// Input format
    #[arg(long, value_enum, default_value_t = InputFormat::Text)]
    pub from: InputFormat,

    /// Report malformed JSONL records to stderr (without their contents)
    #[arg(long)]
    pub debug: bool,
}

impl Cli {
    pub fn parse_localized() -> Self {
        let args: Vec<_> = std::env::args_os().collect();
        i18n::set(i18n::resolve(early_language(&args)));
        let matches = Self::localized_command().get_matches_from(command_language_args(args));
        Self::from_arg_matches(&matches).unwrap_or_else(|e| e.exit())
    }
    pub fn localized_command() -> clap::Command {
        let mut command = Self::command();
        if i18n::current() == i18n::Language::English {
            return command;
        }
        command = command
            .about("在终端中阅读和渲染 Markdown")
            .help_template("{about-with-newline}\n用法：{usage}\n\n{all-args}")
            .subcommand_help_heading("命令")
            .disable_help_flag(true)
            .disable_version_flag(true)
            .arg(
                clap::Arg::new("help")
                    .short('h')
                    .long("help")
                    .action(clap::ArgAction::Help),
            )
            .arg(
                clap::Arg::new("version")
                    .short('V')
                    .long("version")
                    .action(clap::ArgAction::Version),
            );
        for (id, help) in [
            ("file", "要读取的 Markdown 文件；省略时读取标准输入"),
            (
                "language",
                "界面语言：auto（默认）依次跟随 Kaku、系统、英文；en 英文；zh 中文",
            ),
            ("ansi", "直接输出带 ANSI 颜色的文本"),
            ("large_headings", "在 Kaku 中使用实验性双倍 H1 标题"),
            (
                "reader",
                "静态文件阅读模式：inline 内置阅读；native 已配置的 Kaku 集成",
            ),
            ("images", "阅读器图片：auto（默认）自动显示；off 关闭"),
            ("remote_images", "HTTP(S) 图片：on（默认）加载；off 关闭"),
            ("theme", "主题名称，默认 auto"),
            (
                "width",
                "内容列宽；交互阅读默认最多 100 列，其他输出跟随终端，未知时 80 列",
            ),
            (
                "from",
                "输入格式：text（默认）Markdown 文本；claude-json 事件流",
            ),
            ("debug", "输出无效 JSONL 记录的诊断，不包含记录正文"),
            ("help", "显示帮助"),
            ("version", "显示版本"),
        ] {
            command = command.mut_arg(id, |arg| {
                let takes_value = arg.get_action().takes_values();
                arg.help(help)
                    .hide_possible_values(takes_value)
                    .hide_default_value(takes_value)
                    .help_heading(if id == "file" { "参数" } else { "选项" })
            });
        }
        command
            .mut_subcommand("help", |cmd| {
                localized_subcommand(cmd.about("显示命令行帮助"))
            })
            .mut_subcommand("update", |cmd| {
                localized_subcommand(cmd.about("将此二进制安装更新到最新稳定版"))
            })
    }
}

fn localized_subcommand(cmd: clap::Command) -> clap::Command {
    cmd.help_template("{about-with-newline}\n用法：{usage}\n\n{all-args}")
        .disable_help_flag(true)
        .arg(
            clap::Arg::new("help")
                .short('h')
                .long("help")
                .help("显示帮助")
                .help_heading("选项")
                .action(clap::ArgAction::Help),
        )
}

// Clap's args_conflicts_with_subcommands also disables subcommand recognition
// after a global flag. Move only a leading language option past a known command;
// rendering options and explicit `--` paths retain their original semantics.
fn command_language_args(mut args: Vec<std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    let end = match args.get(1).and_then(|s| s.to_str()) {
        Some("--language") => 3,
        Some(arg) if arg.starts_with("--language=") => 2,
        _ => return args,
    };
    if matches!(
        args.get(end).and_then(|s| s.to_str()),
        Some("help" | "update")
    ) {
        args[1..=end].rotate_right(1);
    }
    args
}

fn early_language(args: &[std::ffi::OsString]) -> Option<LanguageChoice> {
    let mut result = None;
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        let value = if arg == "--language" {
            args.next().and_then(|s| s.to_str())
        } else {
            arg.to_str().and_then(|s| s.strip_prefix("--language="))
        };
        if let Some(value) = value {
            result = LanguageChoice::from_str(value, false).ok();
        }
    }
    result
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show command-line help
    Help,
    /// Update this binary-package installation to the latest stable release
    Update,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReaderMode {
    Inline,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ImageMode {
    Auto,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RemoteImages {
    On,
    Off,
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
    fn language_scan_preserves_explicit_file_boundary() {
        let args = |items: &[&str]| {
            items
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            early_language(&args(&["gezi", "--language=zh", "a.md"])),
            Some(LanguageChoice::Zh)
        );
        assert_eq!(
            early_language(&args(&["gezi", "--", "--language=zh"])),
            None
        );
        for language in [i18n::Language::English, i18n::Language::Chinese] {
            let _guard = i18n::scoped(language);
            Cli::localized_command().debug_assert();
            let raw = args(&["gezi", "--language", "zh", "--", "help"]);
            let matches = Cli::localized_command()
                .try_get_matches_from(command_language_args(raw))
                .unwrap();
            let cli = Cli::from_arg_matches(&matches).unwrap();
            assert!(cli.command.is_none());
            assert_eq!(cli.file, Some(PathBuf::from("help")));
        }
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_all_options() {
        let cli = Cli::try_parse_from([
            "gezi",
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
        let cli = Cli::try_parse_from(["gezi"]).unwrap();
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
        assert!(Cli::try_parse_from(["gezi", "--width", "0"]).is_err());
    }

    #[test]
    fn help_is_a_command_and_explicit_help_paths_remain_files() {
        let cli = Cli::try_parse_from(["gezi", "help"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Help)));
        assert!(cli.file.is_none());
        for arguments in [
            vec!["gezi", "--", "help"],
            vec!["gezi", "./help"],
            vec!["gezi", "help.md"],
        ] {
            let cli = Cli::try_parse_from(arguments.clone()).unwrap();
            assert!(cli.command.is_none());
            assert_eq!(cli.file, Some(PathBuf::from(arguments.last().unwrap())));
        }
        assert!(Cli::try_parse_from(["gezi", "help", "README.md"]).is_err());
        assert!(Cli::try_parse_from(["gezi", "README.md", "help"]).is_err());
    }

    #[test]
    fn update_is_a_command_and_explicit_update_paths_remain_files() {
        for name in ["gezi", "ge"] {
            let cli = Cli::try_parse_from([name, "update"]).unwrap();
            assert!(matches!(cli.command, Some(Command::Update)));
            for arguments in [vec![name, "--", "update"], vec![name, "./update"]] {
                let cli = Cli::try_parse_from(arguments.clone()).unwrap();
                assert!(cli.command.is_none());
                assert_eq!(cli.file, Some(PathBuf::from(arguments.last().unwrap())));
            }
            for arguments in [
                vec![name, "update", "notes.md"],
                vec![name, "update", "--ansi"],
            ] {
                assert!(Cli::try_parse_from(arguments).is_err());
            }
        }
    }

    #[test]
    fn width_prefers_explicit_then_terminal_then_fallback() {
        assert_eq!(resolve_width(Some(40), Some(120)), 40);
        assert_eq!(resolve_width(None, Some(120)), 120);
        assert_eq!(resolve_width(None, None), FALLBACK_WIDTH);
    }
}
