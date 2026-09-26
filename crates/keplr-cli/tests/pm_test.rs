//! Black-box lifecycle test: start → list → status → stop, fully
//! isolated via a temp KEPLR_HOME passed per-command (no env races).

use std::path::PathBuf;
use std::process::Command;

fn keplr() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_keplr"))
}

fn scratch(tag: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "keplr-pm-e2e-{}-{}-{tag}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    let home = base.join("home");
    let root = base.join("ws");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    (home, root)
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    l.local_addr().unwrap().port()
}

fn run(home: &PathBuf, args: &[&str]) -> (bool, String) {
    let out = Command::new(keplr())
        .env("KEPLR_HOME", home)
        .args(args)
        .output()
        .unwrap();
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn wait_serving(port: u16) -> bool {
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

#[test]
fn lifecycle_start_list_status_stop() {
    let (home, root) = scratch("cycle");
    let port = free_port().to_string();
    let root_s = root.display().to_string();

    let (ok, out) = run(
        &home,
        &["--root", &root_s, "start", "--name", "e2e", "--port", &port],
    );
    assert!(ok, "start failed: {out}");
    assert!(
        out.contains("started 'e2e'"),
        "unexpected start output: {out}"
    );

    let port_n: u16 = port.parse().unwrap();
    assert!(wait_serving(port_n), "serve never opened port {port_n}");

    let (ok, out) = run(&home, &["list"]);
    assert!(ok, "list failed: {out}");
    assert!(out.contains("e2e"), "list missing instance: {out}");

    let (ok, out) = run(&home, &["status", "--name", "e2e"]);
    assert!(ok, "status failed: {out}");
    assert!(out.contains(&port), "status missing port: {out}");

    let (ok, out) = run(&home, &["stop", "--name", "e2e"]);
    assert!(ok, "stop failed: {out}");
    assert!(
        out.contains("stopped 'e2e'"),
        "unexpected stop output: {out}"
    );

    let (ok, out) = run(&home, &["list"]);
    assert!(ok, "list failed: {out}");
    assert!(!out.contains("e2e"), "instance still listed: {out}");

    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}

#[test]
fn double_start_is_rejected() {
    let (home, root) = scratch("double");
    let port = free_port().to_string();
    let root_s = root.display().to_string();

    let (ok, _) = run(
        &home,
        &["--root", &root_s, "start", "--name", "dup", "--port", &port],
    );
    assert!(ok);
    let (ok, out) = run(
        &home,
        &["--root", &root_s, "start", "--name", "dup", "--port", &port],
    );
    assert!(!ok, "second start should fail");
    assert!(out.contains("already running"), "unexpected output: {out}");

    let _ = run(&home, &["stop", "--name", "dup"]);
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}

#[test]
fn help_lists_lifecycle_commands() {
    let (home, _root) = scratch("help");
    let (ok, out) = run(&home, &["--help"]);
    assert!(ok);
    for cmd in [
        "start",
        "stop",
        "list",
        "status",
        "install",
        "update",
        "downgrade",
        "uninstall",
    ] {
        assert!(out.contains(cmd), "help missing `{cmd}`");
    }
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}
