mod ansi;
mod cli;
#[cfg(test)]
mod snapshots;

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;
use tmd_core::{Theme, render};

use crate::cli::{Cli, resolve_width};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let width = resolve_width(cli.width, terminal_width());

    let input: Box<dyn Read> = match &cli.file {
        Some(path) => match open_file(path) {
            Ok(file) => Box::new(file),
            Err(err) => {
                eprintln!("tmd: {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => Box::new(io::stdin().lock()),
    };
    let md = match read_text(input) {
        Ok(md) => md,
        Err(err) => {
            eprintln!("tmd: {err}");
            return ExitCode::FAILURE;
        }
    };

    // Only the built-in default theme exists until T5.
    let lines = render(&md, width, &Theme::default());
    let hyperlinks = io::stdout().is_terminal();
    match write_out(io::stdout().lock(), &ansi::to_ansi(&lines, hyperlinks)) {
        Ok(()) => ExitCode::SUCCESS,
        // The reader went away (e.g. `tmd file | head`); nothing left to do.
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("tmd: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Opens `path` for reading. Directories are rejected here because on Unix
/// opening one succeeds and only the later read fails, without the path.
fn open_file(path: &Path) -> io::Result<File> {
    let file = File::open(path)?;
    if file.metadata()?.is_dir() {
        return Err(io::ErrorKind::IsADirectory.into());
    }
    Ok(file)
}

/// Reads all input; invalid UTF-8 becomes U+FFFD instead of failing.
fn read_text(mut input: impl Read) -> io::Result<String> {
    let mut bytes = Vec::new();
    input.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Columns of the terminal stdout is attached to, or `None` when stdout is
/// not a terminal.
fn terminal_width() -> Option<u16> {
    if !io::stdout().is_terminal() {
        return None;
    }
    crossterm::terminal::size()
        .ok()
        .map(|(cols, _)| cols)
        .filter(|&cols| cols > 0)
}

fn write_out(mut output: impl Write, text: &str) -> io::Result<()> {
    output.write_all(text.as_bytes())?;
    output.flush()
}
