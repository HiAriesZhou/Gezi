//! Small terminal primitives shared by output modes, without owning a session.
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

// CAN cancels a partial escape; ST closes a partial OSC string.
pub const RESET: &str = "\x18\x1b\\\x1b]8;;\x1b\\\x1b[0m\x1b#5";

pub fn direct_kaku() -> bool {
    is_direct_kaku(
        std::env::var("TERM_PROGRAM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
        std::env::var_os("TMUX").is_some() || std::env::var_os("STY").is_some(),
    )
}

pub fn is_direct_kaku(program: Option<&str>, term: Option<&str>, multiplexed: bool) -> bool {
    program == Some("Kaku")
        && !multiplexed
        && term.is_some_and(|term| {
            term != "dumb" && !term.starts_with("screen") && !term.starts_with("tmux")
        })
}

pub fn write_all_cancelled(
    out: &mut impl Write,
    mut bytes: &[u8],
    cancelled: &AtomicUsize,
) -> io::Result<()> {
    while !bytes.is_empty() {
        if cancelled.load(Ordering::SeqCst) != 0 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        match out.write(bytes) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => bytes = &bytes[n..],
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// Ends the process by `signal` after the caller restored the terminal.
/// Shells stop a script or command list only when a child died by SIGINT;
/// exiting with `128 + signal` would let the next command run.
pub fn terminate_by(signal: i32) -> ! {
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    #[cfg(unix)]
    if signal != signal_hook::consts::SIGQUIT {
        // QUIT's default action dumps core, which macOS records as a crash
        // report; it keeps the plain `128 + signal` exit below.
        let _ = signal_hook::low_level::emulate_default_handler(signal);
    }
    // Reached for QUIT, without signal emulation, or when it fails.
    std::process::exit(128 + signal)
}
