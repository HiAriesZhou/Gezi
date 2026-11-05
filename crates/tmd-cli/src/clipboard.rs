//! Explicit local clipboard writes; this module never reads the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyError {
    Empty,
    Unsupported,
    Backend(&'static str),
}
impl std::fmt::Display for CopyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "代码块为空，未修改剪贴板",
            Self::Unsupported => "此环境暂不支持代码复制（仅本机 macOS）",
            Self::Backend(message) => message,
        })
    }
}

pub(crate) fn copy(text: &str) -> Result<(), CopyError> {
    let remote = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some());
    copy_local(text, cfg!(target_os = "macos") && !remote, write_system)
}

fn copy_local(
    text: &str,
    supported: bool,
    mut write: impl FnMut(&str) -> Result<(), &'static str>,
) -> Result<(), CopyError> {
    if text.is_empty() {
        return Err(CopyError::Empty);
    }
    if !supported {
        return Err(CopyError::Unsupported);
    }
    write(text).map_err(CopyError::Backend)
}

#[cfg(target_os = "macos")]
fn write_system(text: &str) -> Result<(), &'static str> {
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法访问系统剪贴板")?;
    clipboard
        .set_text(text)
        .map_err(|_| "复制失败，未能确认剪贴板写入")
}
#[cfg(not(target_os = "macos"))]
fn write_system(_: &str) -> Result<(), &'static str> {
    Err("此环境暂不支持代码复制")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_remote_requests_never_reach_a_clipboard_backend() {
        for (text, supported, expected) in [
            ("", true, CopyError::Empty),
            ("code", false, CopyError::Unsupported),
        ] {
            assert_eq!(
                copy_local(text, supported, |_| panic!("must not write")),
                Err(expected)
            );
        }
    }
    #[test]
    fn write_once_preserves_payload_and_reports_errors() {
        let text = "{\\rtf1 \t中文\n}\n";
        let mut calls = 0;
        let result = copy_local(text, true, |received| {
            calls += 1;
            assert_eq!(received, text);
            Err("测试写入失败")
        });
        assert_eq!(result, Err(CopyError::Backend("测试写入失败")));
        assert_eq!(calls, 1);
        assert_eq!(
            copy_local(text, true, |received| {
                assert_eq!(received, text);
                Ok(())
            }),
            Ok(())
        );
    }
}
