mod cli;

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;

use crate::cli::{Cli, resolve_width};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let _width = resolve_width(cli.width, terminal_width());

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

    // Rendering comes in T2; for now the input is passed through unchanged.
    match passthrough(input, io::stdout().lock()) {
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

fn passthrough(mut input: impl Read, mut output: impl Write) -> io::Result<()> {
    io::copy(&mut input, &mut output)?;
    output.flush()
}
