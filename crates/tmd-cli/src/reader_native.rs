//! Versioned, opt-in Kaku adapter with private temporary-file replies.
//! Late replies never enter terminal input.
use std::collections::VecDeque;
use std::io::{self, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event};

pub fn direct_kaku() -> bool {
    std::env::var("TERM_PROGRAM").as_deref() == Ok("Kaku")
        && std::env::var_os("TMUX").is_none()
        && std::env::var_os("STY").is_none()
        && std::env::var("TERM")
            .is_ok_and(|term| !term.starts_with("screen") && !term.starts_with("tmux"))
}

pub fn token() -> String {
    format!(
        "{:x}{:x}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

pub fn notify(out: &mut impl Write, action: &str, token: &str) -> io::Result<()> {
    send(out, &format!("v1:{action}:{token}"))
}

fn send(out: &mut impl Write, value: &str) -> io::Result<()> {
    let payload = base64(value.as_bytes());
    write!(out, "\x1b]1337;SetUserVar=TMD_READER={payload}\x07")?;
    out.flush()
}

#[cfg(unix)]
struct ReplyFile {
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl ReplyFile {
    fn new(token: &str) -> io::Result<Self> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let dir = std::env::temp_dir().join(format!("tmd-reader-{token}"));
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
        let reply = Self {
            path: dir.join("reply"),
            dir,
        };
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&reply.path)?
            .write_all(format!("tmd-reader-v1:{token}\n").as_bytes())?;
        Ok(reply)
    }
}

#[cfg(unix)]
impl Drop for ReplyFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.dir);
    }
}

#[cfg(unix)]
pub fn probe(
    out: &mut impl Write,
    token: &str,
    queued: &mut VecDeque<Event>,
) -> io::Result<Option<usize>> {
    let file = match ReplyFile::new(token) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    // Replies never enter terminal input. A late callback sees a removed
    // private directory and cannot recreate the reply or pollute a shell.
    send(out, &format!("v1:probe:{token}:{}", file.path.display()))?;
    let deadline = Instant::now() + Duration::from_millis(500);
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        if let Ok(text) = std::fs::read_to_string(&file.path)
            && let Some(capacity) = reply(&text, token)
        {
            if event::poll(Duration::ZERO)? {
                queued.push_back(event::read()?);
                return Ok(None);
            }
            return Ok(Some(capacity));
        }
        if event::poll(remaining.min(Duration::from_millis(20)))? {
            queued.push_back(event::read()?);
            return Ok(None);
        }
    }
    Ok(None)
}

#[cfg(not(unix))]
pub fn probe(_: &mut impl Write, _: &str, _: &mut VecDeque<Event>) -> io::Result<Option<usize>> {
    Ok(None)
}

fn reply(text: &str, token: &str) -> Option<usize> {
    text.strip_prefix(&format!("tmd-reader-v1:{token}\n"))?
        .strip_suffix('\n')?
        .parse()
        .ok()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapter_reply_requires_matching_version_token_and_capacity() {
        assert_eq!(reply("tmd-reader-v1:abc\n10000\n", "abc"), Some(10000));
        for value in [
            "tmd-reader-v1:other\n10000\n",
            "tmd-reader-v2:abc\n10000\n",
            "tmd-reader-v1:abc\n-1\n",
            "tmd-reader-v1:abc\n1x\n",
        ] {
            assert_eq!(reply(value, "abc"), None);
        }
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        let mut out = Vec::new();
        notify(&mut out, "probe", "abc").unwrap();
        assert_eq!(out, b"\x1b]1337;SetUserVar=TMD_READER=djE6cHJvYmU6YWJj\x07");
    }

    #[cfg(unix)]
    #[test]
    fn reply_channel_is_private_and_removed_before_a_late_callback() {
        use std::os::unix::fs::PermissionsExt;
        let token = token();
        let channel = ReplyFile::new(&token).unwrap();
        let path = channel.path.clone();
        assert_eq!(
            std::fs::metadata(&channel.dir)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut reply_file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(reply_file, "10000").unwrap();
        assert_eq!(
            reply(&std::fs::read_to_string(&path).unwrap(), &token),
            Some(10000)
        );
        drop(reply_file);
        drop(channel);
        assert!(!path.parent().unwrap().exists());
        assert!(
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)
                .is_err()
        );
    }
}
