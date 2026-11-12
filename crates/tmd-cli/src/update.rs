//! Update the current binary-package installation using the bundled installer.
use std::path::Path;
use std::process::{Command, Stdio};

const RELEASES: &str = "https://github.com/HiAriesZhou/Gezi/releases";
const INSTALLER: &str = include_str!("../../../install.sh");

type Version = [u64; 3];

pub fn run() -> Result<(), String> {
    let latest = latest_tag()?;
    let installed = version(env!("CARGO_PKG_VERSION"))?;
    if version(&latest[1..])? <= installed {
        println!(
            "No update needed: Gezi {} (latest stable: {latest}).",
            env!("CARGO_PKG_VERSION")
        );
        return Ok(());
    }
    let executable = std::env::current_exe()
        .map_err(|err| format!("cannot locate the running Gezi executable: {err}"))?;
    let directory = installation_dir(&executable)?;
    println!("Updating Gezi {} to {latest}...", env!("CARGO_PKG_VERSION"));
    let status = Command::new("/bin/sh")
        .args(["-c", INSTALLER, "gezi-update"])
        .env("GEZI_VERSION", &latest)
        .env("GEZI_INSTALL_DIR", directory)
        .stdin(Stdio::null())
        .status()
        .map_err(|err| format!("could not start the Gezi installer: {err}"))?;
    if !status.success() {
        return Err(format!(
            "Gezi update failed ({status}); see the installer message above."
        ));
    }
    Ok(())
}

fn latest_tag() -> Result<String, String> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "10",
            "--max-time",
            "120",
            "--retry",
            "2",
            "--output",
            "/dev/null",
            "--write-out",
            "%{url_effective}",
        ])
        .arg(format!("{RELEASES}/latest"))
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|err| format!("could not check for updates: {err}; curl is required"))?;
    if !output.status.success() {
        return Err(format!("could not check for updates ({})", output.status));
    }
    let url = std::str::from_utf8(&output.stdout)
        .map_err(|_| "invalid latest-release address".to_owned())?;
    tag_from_url(url).map(str::to_owned)
}

fn tag_from_url(url: &str) -> Result<&str, String> {
    let tag = url
        .strip_prefix(&format!("{RELEASES}/tag/"))
        .ok_or("unexpected latest-release address")?;
    let number = tag.strip_prefix('v').ok_or("invalid stable-release tag")?;
    version(number)?;
    Ok(tag)
}

fn version(text: &str) -> Result<Version, String> {
    let invalid = || format!("invalid stable version: {text:?}");
    let parts: Vec<u64> = text
        .split('.')
        .map(|part| {
            if part.is_empty()
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return Err(invalid());
            }
            part.parse().map_err(|_| invalid())
        })
        .collect::<Result<_, _>>()?;
    parts.try_into().map_err(|_| invalid())
}

fn installation_dir(executable: &Path) -> Result<&Path, String> {
    if !matches!(
        executable.file_name().and_then(|name| name.to_str()),
        Some("gezi" | "ge")
    ) {
        return Err("the running executable must be named gezi or ge to update it".to_owned());
    }
    executable
        .parent()
        .ok_or_else(|| "cannot locate the Gezi installation directory".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_versions_are_compared_numerically() {
        assert!(version("0.1.9").unwrap() < version("0.1.10").unwrap());
        assert!(version("0.9.99").unwrap() < version("1.0.0").unwrap());
        assert_eq!(version("0.1.8").unwrap(), version("0.1.8").unwrap());
    }

    #[test]
    fn latest_address_must_identify_an_exact_stable_tag_in_this_repository() {
        assert_eq!(
            tag_from_url(&format!("{RELEASES}/tag/v0.1.8")).unwrap(),
            "v0.1.8"
        );
        for invalid in [
            "",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.2.-3",
            "1.2.3-rc.1",
            "1.2.3+build",
            "1.2.3?x",
            "1.2.3\n",
            "1.2.3/path",
            "1.2.18446744073709551616",
        ] {
            assert!(version(invalid).is_err(), "{invalid:?}");
            assert!(tag_from_url(&format!("{RELEASES}/tag/v{invalid}")).is_err());
        }
        assert!(tag_from_url("https://example.invalid/releases/tag/v0.1.8").is_err());
        assert!(tag_from_url(&format!("{RELEASES}/tag/0.1.8")).is_err());
    }
}
