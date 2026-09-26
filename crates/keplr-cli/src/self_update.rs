//! Install / update / downgrade / uninstall via GitHub releases.
//!
//! Release assets are raw binaries named `keplr-{target}` (plus `.exe`
//! on Windows), e.g. `keplr-x86_64-unknown-linux-gnu`. No archives, so
//! no tar/gzip handling is needed. Downloads use `curl` (or `wget`
//! fallback) — both are preinstalled on every platform keplr targets.
//! Private repos (like this one) need `GITHUB_TOKEN` or `GH_TOKEN`.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const OWNER: &str = "NaveenSingh9999";
const REPO: &str = "keplr";

/// Asset suffix for this build's platform, or an honest bail for
/// platforms with no prebuilt binary.
pub fn target_triple() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("android", "aarch64") => "aarch64-linux-android",
        (os, arch) => bail!(
            "no prebuilt binary for {os}/{arch} — build from source: cargo install -p keplr-cli"
        ),
    })
}

fn exe_suffix() -> &'static str {
    if std::env::consts::OS == "windows" {
        ".exe"
    } else {
        ""
    }
}

/// "latest" (any case) and "" pass through; "0.1.0" → "v0.1.0".
pub fn normalize_version(v: &str) -> String {
    let v = v.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("latest") {
        return "latest".to_string();
    }
    let v = v
        .strip_prefix('v')
        .or_else(|| v.strip_prefix('V'))
        .unwrap_or(v);
    format!("v{v}")
}

fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .or_else(|_| std::env::var("GH_TOKEN"))
        .ok()
        .filter(|t| !t.trim().is_empty())
}

/// Fetch a URL to memory with curl, falling back to wget.
fn fetch(url: &str) -> Result<Vec<u8>> {
    let mut args = vec![
        "-sSfL".to_string(),
        "--proto".to_string(),
        "=https".to_string(),
    ];
    if let Some(tok) = github_token() {
        args.push("-H".to_string());
        args.push(format!("Authorization: Bearer {tok}"));
    }
    args.push(url.to_string());
    match Command::new("curl").args(&args).output() {
        Ok(o) if o.status.success() => return Ok(o.stdout),
        _ => {}
    }
    let mut wargs = vec!["-qO-".to_string()];
    if let Some(tok) = github_token() {
        wargs.push(format!("--header=Authorization: Bearer {tok}"));
    }
    wargs.push(url.to_string());
    match Command::new("wget").args(&wargs).output() {
        Ok(o) if o.status.success() => return Ok(o.stdout),
        _ => {}
    }
    bail!("fetch failed for {url}: need `curl` or `wget` on PATH (private repos also need GITHUB_TOKEN)");
}

fn latest_tag() -> Result<String> {
    let url = format!("https://api.github.com/repos/{OWNER}/{REPO}/releases/latest");
    let bytes = fetch(&url).context(
        "GitHub API request failed — private repos need GITHUB_TOKEN in the environment",
    )?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).context("parse GitHub release JSON")?;
    v.get("tag_name")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .context("no tag_name in response — has any release been published yet?")
}

fn resolve_tag(version: &str) -> Result<String> {
    if version == "latest" {
        latest_tag()
    } else {
        Ok(normalize_version(version))
    }
}

fn bin_dir() -> Result<PathBuf> {
    let dir = crate::pm::keplr_dir()?.join("bin");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn dest_path() -> Result<PathBuf> {
    Ok(bin_dir()?.join(format!("keplr{}", exe_suffix())))
}

/// Symlink ~/.local/bin/keplr at the managed binary. Never deletes a
/// real (non-symlink) file the user put there themselves.
#[cfg(unix)]
fn link_into_path(dest: &Path) -> Result<()> {
    let Ok(home) = std::env::var("HOME") else {
        return Ok(());
    };
    if home.is_empty() {
        return Ok(());
    }
    let link = PathBuf::from(&home).join(".local/bin/keplr");
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::symlink_metadata(&link) {
        Ok(md) if md.file_type().is_symlink() => {
            if std::fs::read_link(&link)
                .map(|p| p == dest)
                .unwrap_or(false)
            {
                return Ok(());
            }
            std::fs::remove_file(&link)?;
            std::os::unix::fs::symlink(dest, &link)?;
            println!("linked {}", link.display());
        }
        Ok(_) => {
            println!(
                "note: {} is a real file (not our link) — leaving it alone; managed binary is {}",
                link.display(),
                dest.display()
            );
            return Ok(());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::os::unix::fs::symlink(dest, &link)?;
            println!("linked {}", link.display());
        }
        Err(e) => return Err(e).context("inspect launcher link"),
    }
    let bindir = format!("{home}/.local/bin");
    if std::env::var("PATH")
        .map(|p| !p.split(':').any(|d| d == bindir))
        .unwrap_or(true)
    {
        println!("note: ~/.local/bin is not on PATH — add: export PATH=\"$HOME/.local/bin:$PATH\"");
    }
    Ok(())
}

#[cfg(not(unix))]
fn link_into_path(dest: &Path) -> Result<()> {
    // No symlinks without privileges on Windows: point at the folder instead.
    println!("installed to {}", dest.display());
    if let Some(dir) = dest.parent() {
        println!(
            "note: add {} to your PATH (e.g. setx PATH \"%PATH%;{}\")",
            dir.display(),
            dir.display()
        );
    }
    Ok(())
}

/// Download a release binary, smoke-test it (`--help` must exit 0),
/// then atomically swap it in. Keeps the previous binary as `keplr.bak`.
pub fn install(version: &str) -> Result<()> {
    let tag = resolve_tag(&normalize_version(version))?;
    let triple = target_triple()?;
    let asset = format!("keplr-{triple}{}", exe_suffix());
    let url = format!("https://github.com/{OWNER}/{REPO}/releases/download/{tag}/{asset}");
    let dest = dest_path()?;
    println!("downloading {asset} ({tag}) …");
    let bytes = fetch(&url)
        .with_context(|| format!("download {asset} for {tag} — does that release/asset exist?"))?;
    if bytes.is_empty() {
        bail!("downloaded file is empty");
    }
    let dir = dest.parent().unwrap().to_path_buf();
    let tmp = dir.join(format!(".keplr-{}.tmp", std::process::id()));
    std::fs::write(&tmp, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    let smoke = Command::new(&tmp)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !smoke {
        let _ = std::fs::remove_file(&tmp);
        bail!("downloaded binary failed its smoke test (--help) — wrong platform asset?");
    }
    if dest.exists() {
        let bak = dir.join(format!("keplr.bak{}", exe_suffix()));
        let _ = std::fs::remove_file(&bak);
        std::fs::rename(&dest, &bak)?;
    }
    std::fs::rename(&tmp, &dest)?;
    link_into_path(&dest)?;
    println!("installed keplr {tag} → {}", dest.display());
    Ok(())
}

/// Stop all instances, swap the binary, restart them. The swap itself
/// is an atomic rename; if the download fails the old binary is
/// untouched and instances come back on it.
pub fn update(version: &str) -> Result<()> {
    let stopped = crate::pm::stop_all()?;
    let exe = std::env::current_exe().context("locate current binary")?;
    if let Err(e) = install(version) {
        eprintln!("keplr: update failed: {e:#} — restarting old instances");
        let _ = crate::pm::restart_specs(&stopped, &exe);
        return Err(e);
    }
    if let Err(e) = crate::pm::restart_specs(&stopped, &exe) {
        eprintln!("keplr: some instances failed to restart: {e:#}");
    }
    println!("updated; restarted {} instance(s)", stopped.len());
    Ok(())
}

pub fn downgrade(version: &str) -> Result<()> {
    if version.trim().is_empty() || version.eq_ignore_ascii_case("latest") {
        bail!("downgrade needs an explicit older version: `keplr downgrade 0.1.0`");
    }
    update(version)
}

/// Stop everything, remove the launcher link (only if it is ours) and
/// the whole `~/.keplr` state dir.
pub fn uninstall(confirmed: bool) -> Result<()> {
    if !confirmed {
        eprint!(
            "uninstall keplr (stop all instances, delete ~/.keplr and the launcher link)? [y/N] "
        );
        use std::io::Write as _;
        std::io::stdout().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if !matches!(line.trim().to_lowercase().as_str(), "y" | "yes") {
            println!("aborted");
            return Ok(());
        }
    }
    let _ = crate::pm::stop_all();
    #[cfg(unix)]
    if let Ok(home) = std::env::var("HOME") {
        let link = PathBuf::from(home).join(".local/bin/keplr");
        if let Ok(target) = std::fs::read_link(&link) {
            if let Ok(kd) = crate::pm::keplr_dir() {
                if target.starts_with(&kd) {
                    let _ = std::fs::remove_file(&link);
                    println!("removed {}", link.display());
                }
            }
        }
    }
    let dir = crate::pm::keplr_dir()?;
    if dir.exists() {
        // The running binary cannot delete itself on Windows: if we live
        // inside the managed dir, remove state only and say so honestly.
        let self_managed = std::env::current_exe()
            .ok()
            .map(|e| e.starts_with(&dir))
            .unwrap_or(false);
        if self_managed {
            let _ = std::fs::remove_file(dir.join("instances.json"));
            let _ = std::fs::remove_dir_all(dir.join("logs"));
            println!(
                "removed state; delete {} by hand after exiting (it holds the running binary)",
                dir.display()
            );
        } else if let Err(e) = std::fs::remove_dir_all(&dir) {
            return Err(e).with_context(|| format!("remove {}", dir.display()));
        } else {
            println!("removed {}", dir.display());
        }
    }
    println!("uninstalled. (Cargo-installed copies need: cargo uninstall -p keplr-cli)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_normalize() {
        assert_eq!(normalize_version("latest"), "latest");
        assert_eq!(normalize_version("LATEST"), "latest");
        assert_eq!(normalize_version(""), "latest");
        assert_eq!(normalize_version("0.1.0"), "v0.1.0");
        assert_eq!(normalize_version("v0.1.0"), "v0.1.0");
        assert_eq!(normalize_version("  v2.3  "), "v2.3");
    }

    #[test]
    fn triple_is_supported_here() {
        // Must not bail on the platform running the test suite.
        let t = target_triple().unwrap();
        assert!(t.contains(std::env::consts::ARCH));
    }

    #[test]
    fn downgrade_rejects_latest() {
        // No network involved: validation happens before any fetch.
        assert!(downgrade("latest").is_err());
        assert!(downgrade("").is_err());
    }
}
