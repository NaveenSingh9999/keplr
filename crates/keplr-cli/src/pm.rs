//! Background instance manager: `keplr start / stop / list / status`.
//!
//! Design: one JSON registry at `~/.keplr/instances.json` (overridable
//! via `KEPLR_HOME` for tests). `start` spawns the current binary as a
//! detached `serve` child with logs redirected, records its PID, and
//! exits — no systemd, no tmux, works on Termux/macOS/Linux. `stop`
//! sends SIGTERM, waits, then SIGKILL. Dead PIDs are pruned on every read.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub pid: u32,
    pub port: u16,
    pub root: String,
    pub bind: String,
    pub allow_open_lan: bool,
    pub started_at: u64,
    pub token_gate: bool,
}

pub type Registry = BTreeMap<String, Instance>;

/// Home for keplr state. `KEPLR_HOME` wins (tests use a temp dir),
/// otherwise the OS home directory.
pub fn home_dir() -> Result<PathBuf> {
    if let Ok(h) = std::env::var("KEPLR_HOME") {
        if !h.trim().is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    #[cfg(unix)]
    if let Ok(h) = std::env::var("HOME") {
        if !h.trim().is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    #[cfg(windows)]
    if let Ok(h) = std::env::var("USERPROFILE") {
        if !h.trim().is_empty() {
            return Ok(PathBuf::from(h));
        }
    }
    bail!("cannot locate a home directory — set KEPLR_HOME or HOME")
}

pub fn keplr_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".keplr"))
}

pub fn registry_path() -> Result<PathBuf> {
    Ok(keplr_dir()?.join("instances.json"))
}

pub fn log_path(name: &str) -> Result<PathBuf> {
    Ok(keplr_dir()?.join("logs").join(format!("{name}.log")))
}

pub fn load_registry() -> Result<Registry> {
    let path = registry_path()?;
    match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
        Ok(text) => {
            serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
        }
    }
}

pub fn save_registry(reg: &Registry) -> Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(reg)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// True if a process with this PID exists (same-user check is enough:
/// we only ever manage processes we started).
pub fn alive(pid: u32) -> bool {
    // Guard first: pid 0 is special to kill(2), and values above i32::MAX
    // would wrap to negative PIDs with process-group semantics.
    // (u32::MAX as i32 == -1 == "every process" — must never query that.)
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    #[cfg(unix)]
    {
        // kill(pid, 0) performs no signal delivery; 0 = exists.
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// Drop entries whose PIDs are dead. Returns true if anything changed.
pub fn prune(reg: &mut Registry) -> bool {
    let before = reg.len();
    reg.retain(|_, inst| alive(inst.pid));
    reg.len() != before
}

pub fn port_free(host: &str, port: u16) -> bool {
    std::net::TcpListener::bind((host, port)).is_ok()
}

/// Wait up to `timeout` for a port to become free (covers the gap
/// between killing an old server and the kernel releasing its socket).
fn wait_port_free(host: &str, port: u16, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if port_free(host, port) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    port_free(host, port)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn default_name(root: &Path) -> String {
    root.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "keplr".to_string())
}

pub fn fmt_uptime(secs: u64) -> String {
    if secs < 60 {
        return format!("{secs}s");
    }
    let m = secs / 60;
    if m < 60 {
        return format!("{m}m{:02}s", secs % 60);
    }
    let h = m / 60;
    if h < 48 {
        return format!("{h}h{:02}m", m % 60);
    }
    format!("{}d{:02}h", h / 24, h % 24)
}

fn tail_file(path: &Path, n: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let skip = lines.len().saturating_sub(n);
    lines[skip..].join("\n")
}

pub struct StartSpec {
    pub name: String,
    pub port: Option<u16>,
    pub root: PathBuf,
    pub token: String,
    pub bind: String,
    pub allow_open_lan: bool,
}

/// Spawn a detached `serve` child, record it, and return its entry.
/// `exe` is the keplr binary to run (normally `std::env::current_exe()`).
pub fn start(spec: &StartSpec, exe: &Path) -> Result<Instance> {
    if spec.name.trim().is_empty() {
        bail!("instance name must not be empty");
    }
    if spec.name.contains('/') || spec.name.contains('\\') || spec.name.contains('\0') {
        bail!("instance name must be a plain label, got '{}'", spec.name);
    }
    let mut reg = load_registry()?;
    if prune(&mut reg) {
        save_registry(&reg)?;
    }
    if let Some(inst) = reg.get(&spec.name) {
        bail!(
            "instance '{}' is already running (pid {}, port {}) — `keplr stop --name {}` first",
            spec.name,
            inst.pid,
            inst.port,
            spec.name
        );
    }
    let root = spec
        .root
        .canonicalize()
        .unwrap_or_else(|_| spec.root.clone());
    if !root.is_dir() {
        bail!("root is not a directory: {}", root.display());
    }
    let port = match spec.port {
        Some(p) => {
            if !wait_port_free(&spec.bind, p, std::time::Duration::from_secs(2)) {
                bail!(
                    "port {p} on {} is already in use — pick another with --port",
                    spec.bind
                );
            }
            p
        }
        None => {
            let mut p: u16 = 7137;
            loop {
                if port_free(&spec.bind, p) {
                    break p;
                }
                p = p.checked_add(1).context("no free port available")?;
            }
        }
    };

    let log = log_path(&spec.name)?;
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log_out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .with_context(|| format!("open log {}", log.display()))?;
    let log_err = log_out.try_clone()?;

    let mut cmd = Command::new(exe);
    cmd.arg("--root")
        .arg(&root)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--bind")
        .arg(&spec.bind);
    if !spec.token.is_empty() {
        cmd.arg("--token").arg(&spec.token);
    }
    if spec.allow_open_lan {
        cmd.arg("--allow-open-lan");
    }
    cmd.stdin(Stdio::null()).stdout(log_out).stderr(log_err);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(0x00000008); // DETACHED_PROCESS
    }
    // Intentionally not waited on: the child outlives us (reparented to
    // init / detached process group) while the CLI exits immediately.
    let pid = cmd
        .spawn()
        .with_context(|| format!("spawn {}", exe.display()))?
        .id();

    std::thread::sleep(std::time::Duration::from_millis(600));
    if !alive(pid) {
        bail!(
            "serve exited immediately; last log lines from {}:\n{}",
            log.display(),
            tail_file(&log, 20)
        );
    }
    let token_gate = !keplr_serve::resolve_token(&root, &spec.token).is_empty();
    let inst = Instance {
        pid,
        port,
        root: root.display().to_string(),
        bind: spec.bind.clone(),
        allow_open_lan: spec.allow_open_lan,
        started_at: now_secs(),
        token_gate,
    };
    reg.insert(spec.name.clone(), inst.clone());
    save_registry(&reg)?;
    println!(
        "started '{}' on {}:{} (pid {}) — logs {}",
        spec.name,
        spec.bind,
        port,
        pid,
        log.display()
    );
    Ok(inst)
}

#[cfg(unix)]
fn signal(pid: u32, sig: libc::c_int) -> Result<()> {
    let r = unsafe { libc::kill(pid as libc::pid_t, sig) };
    if r != 0 {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::ESRCH) {
            return Ok(()); // already gone
        }
        return Err(e).context("signal delivery failed");
    }
    Ok(())
}

#[cfg(windows)]
fn terminate_tree(pid: u32, force: bool) -> Result<()> {
    let mut args = vec!["/PID".to_string(), pid.to_string(), "/T".to_string()];
    if force {
        args.push("/F".to_string());
    }
    let st = Command::new("taskkill")
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match st {
        Ok(s) if s.success() => Ok(()),
        _ if !alive(pid) => Ok(()),
        _ => bail!("taskkill failed for pid {pid}"),
    }
}

pub fn stop(name: &str) -> Result<()> {
    let mut reg = load_registry()?;
    let inst = reg
        .get(name)
        .with_context(|| format!("no instance named '{name}' — see `keplr list`"))?
        .clone();
    if alive(inst.pid) {
        #[cfg(unix)]
        signal(inst.pid, libc::SIGTERM)?;
        #[cfg(windows)]
        terminate_tree(inst.pid, false)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while alive(inst.pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if alive(inst.pid) {
            #[cfg(unix)]
            signal(inst.pid, libc::SIGKILL)?;
            #[cfg(windows)]
            terminate_tree(inst.pid, true)?;
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        if alive(inst.pid) {
            bail!(
                "could not kill pid {} — remove it by hand, then `keplr stop` again",
                inst.pid
            );
        }
    }
    reg.remove(name);
    save_registry(&reg)?;
    println!("stopped '{name}'");
    Ok(())
}

/// Stop the single running instance (convenience for `keplr stop` with no args).
pub fn stop_single() -> Result<()> {
    let mut reg = load_registry()?;
    if prune(&mut reg) {
        save_registry(&reg)?;
    }
    match reg.len() {
        0 => bail!("no keplr instances running"),
        1 => {
            let name = reg.keys().next().cloned().unwrap();
            stop(&name)
        }
        _ => bail!(
            "multiple instances running — pass --name ({}) or --all",
            reg.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Stop every instance; returns their specs for restart flows.
pub fn stop_all() -> Result<Vec<(String, Instance)>> {
    let mut reg = load_registry()?;
    if prune(&mut reg) {
        save_registry(&reg)?;
    }
    let names: Vec<String> = reg.keys().cloned().collect();
    let mut stopped = Vec::with_capacity(names.len());
    for n in &names {
        if let Some(inst) = reg.get(n).cloned() {
            stop(n)?;
            stopped.push((n.clone(), inst));
        }
    }
    Ok(stopped)
}

/// Restart previously-stopped instances with a (possibly new) binary.
/// Tokens are NOT stored: serve re-resolves them from each root's config.
pub fn restart_specs(stopped: &[(String, Instance)], exe: &Path) -> Result<()> {
    for (name, inst) in stopped {
        let spec = StartSpec {
            name: name.clone(),
            port: Some(inst.port),
            root: PathBuf::from(&inst.root),
            token: String::new(),
            bind: inst.bind.clone(),
            allow_open_lan: inst.allow_open_lan,
        };
        if let Err(e) = start(&spec, exe) {
            eprintln!("keplr: restart of '{name}' failed: {e:#}");
        }
    }
    Ok(())
}

pub fn list() -> Result<()> {
    let mut reg = load_registry()?;
    if prune(&mut reg) {
        save_registry(&reg)?;
    }
    if reg.is_empty() {
        println!("no keplr instances running — `keplr start --help`");
        return Ok(());
    }
    println!(
        "{:<12} {:<6} {:<8} {:<9} ROOT",
        "NAME", "PORT", "PID", "UPTIME"
    );
    for (name, inst) in &reg {
        let gate = if inst.token_gate { " [token]" } else { "" };
        println!(
            "{:<12} {:<6} {:<8} {:<9} {}{}",
            name,
            inst.port,
            inst.pid,
            fmt_uptime(now_secs().saturating_sub(inst.started_at)),
            inst.root,
            gate
        );
    }
    Ok(())
}

fn rss(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("VmRSS:") {
                return Some(v.trim().to_string());
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

pub fn status(name: Option<&str>) -> Result<()> {
    let mut reg = load_registry()?;
    if prune(&mut reg) {
        save_registry(&reg)?;
    }
    let (name, inst) = match name {
        Some(n) => (
            n.to_string(),
            reg.get(n)
                .with_context(|| format!("no instance named '{n}' — see `keplr list`"))?
                .clone(),
        ),
        None => match reg.len() {
            0 => bail!("no keplr instances running"),
            1 => {
                let (n, i) = reg.iter().next().unwrap();
                (n.clone(), i.clone())
            }
            _ => bail!(
                "multiple instances running — pass --name ({})",
                reg.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        },
    };
    println!("name:      {name}");
    println!("pid:       {}", inst.pid);
    println!("endpoint:  http://{}:{}", inst.bind, inst.port);
    println!("root:      {}", inst.root);
    println!(
        "uptime:    {}",
        fmt_uptime(now_secs().saturating_sub(inst.started_at))
    );
    println!(
        "token:     {}",
        if inst.token_gate { "enabled" } else { "off" }
    );
    println!(
        "rss:       {}",
        rss(inst.pid).unwrap_or_else(|| "n/a".to_string())
    );
    println!("log:       {}", log_path(&name)?.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_home(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "keplr-pm-unit-{}-{}-{tag}",
            std::process::id(),
            now_secs()
        ))
    }

    #[test]
    fn registry_roundtrip() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = temp_home("roundtrip");
        std::env::set_var("KEPLR_HOME", &home);
        let mut reg = Registry::new();
        reg.insert(
            "dev".to_string(),
            Instance {
                pid: 1,
                port: 7137,
                root: "/tmp/x".to_string(),
                bind: "127.0.0.1".to_string(),
                allow_open_lan: false,
                started_at: 100,
                token_gate: true,
            },
        );
        save_registry(&reg).unwrap();
        let back = load_registry().unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back["dev"].port, 7137);
        assert!(back["dev"].token_gate);
        std::env::remove_var("KEPLR_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn stale_entries_are_pruned() {
        let _g = ENV_LOCK.lock().unwrap();
        let home = temp_home("prune");
        std::env::set_var("KEPLR_HOME", &home);
        let mut reg = Registry::new();
        // u32::MAX can never be a live PID: kill() fails with ESRCH.
        reg.insert(
            "ghost".to_string(),
            Instance {
                pid: u32::MAX,
                port: 7137,
                root: "/tmp/x".to_string(),
                bind: "127.0.0.1".to_string(),
                allow_open_lan: false,
                started_at: 100,
                token_gate: false,
            },
        );
        assert!(!alive(u32::MAX));
        assert!(prune(&mut reg));
        assert!(reg.is_empty());
        std::env::remove_var("KEPLR_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn port_check_matches_reality() {
        let probe = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = probe.local_addr().unwrap().port();
        assert!(!port_free("127.0.0.1", port));
        drop(probe);
        assert!(port_free("127.0.0.1", port));
    }

    #[test]
    fn uptime_formats() {
        assert_eq!(fmt_uptime(5), "5s");
        assert_eq!(fmt_uptime(90), "1m30s");
        assert_eq!(fmt_uptime(3700), "1h01m");
        assert_eq!(fmt_uptime(90000), "25h00m");
        assert_eq!(fmt_uptime(176400), "2d01h");
    }

    #[test]
    fn default_name_falls_back() {
        assert_eq!(default_name(Path::new("/home/u/LAML")), "LAML");
        assert_eq!(default_name(Path::new("/")), "keplr");
    }
}
