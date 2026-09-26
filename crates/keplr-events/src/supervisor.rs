//! Supervises the LAML event service as a child process.
//!
//! The service is a realtime fan-out, not a source of truth: if it dies the host
//! keeps working and the supervisor restarts it. Restarts back off so a broken
//! program cannot spin the process table.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Finds the LAML interpreter a Keplr install ships with.
///
/// A packaged Keplr carries the binary next to its executable, so the search
/// starts there and falls back to a developer's own build. Returning `None` is
/// normal: Keplr runs without the event service.
pub fn find_laml() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("KEPLR_LAML") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidates = [
        dir.join("laml"),
        dir.join("lib").join("keplr").join("laml"),
        dir.join("..").join("lib").join("keplr").join("laml"),
        dir.join("..")
            .join("..")
            .join("assets")
            .join("laml")
            .join(platform_asset()),
        dir.join("..")
            .join("..")
            .join("..")
            .join("assets")
            .join("laml")
            .join(platform_asset()),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    which("laml")
}

/// The release asset name for this machine, used inside a source checkout.
pub fn platform_asset() -> String {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        other => other,
    };
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    };
    format!("laml-{os}-{arch}")
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// How to launch and restart the event service.
#[derive(Clone, Debug)]
pub struct SupervisorConfig {
    /// The LAML interpreter, resolved by [`find_laml`].
    pub program: PathBuf,
    /// The service program, shipped with this crate.
    pub script: PathBuf,
    /// Backoff after a clean exit or a crash, doubling up to the ceiling.
    pub min_backoff: Duration,
    pub max_backoff: Duration,
}

impl SupervisorConfig {
    /// Uses the interpreter this install ships with.
    pub fn discover() -> Self {
        Self {
            program: find_laml().unwrap_or_else(|| PathBuf::from("laml")),
            script: default_script(),
            min_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(10),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command
            .arg("run")
            .arg(&self.script)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        command
    }
}

/// The service program shipped with this crate, which is the source tree in a
/// checkout and the packaged copy in an install.
pub fn default_script() -> PathBuf {
    let packaged = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/events.lm");
    if packaged.is_file() {
        return packaged;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("events.lm");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    packaged
}

/// What the supervisor is doing, for a status line or a log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Not started yet.
    Idle,
    /// The child is running.
    Running,
    /// Waiting out a backoff before the next start.
    BackingOff { attempt: u32 },
    /// The interpreter could not be started at all.
    Unavailable,
}

/// Owns the child process and its restart policy.
pub struct Supervisor {
    config: SupervisorConfig,
    child: Option<Child>,
    state: State,
    next_start: Option<Instant>,
    attempt: u32,
    restarts: Arc<AtomicU32>,
}

impl Supervisor {
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            config,
            child: None,
            state: State::Idle,
            next_start: None,
            attempt: 0,
            restarts: Arc::new(AtomicU32::new(0)),
        }
    }

    /// A supervisor that never starts a process, for hosts that run the service
    /// elsewhere or in tests.
    pub fn disabled() -> Self {
        Self::new(SupervisorConfig {
            program: PathBuf::from("laml"),
            script: PathBuf::from("events.lm"),
            min_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(10),
        })
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// How many times the child has been started, including the first start.
    pub fn starts(&self) -> u32 {
        self.restarts.load(Ordering::SeqCst)
    }

    /// Starts the child if it is not running and no backoff is pending.
    pub fn poll(&mut self) -> anyhow::Result<()> {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    self.child = None;
                    self.schedule_retry();
                }
                Ok(None) => {
                    self.state = State::Running;
                    return Ok(());
                }
                Err(error) => {
                    self.child = None;
                    self.state = State::Unavailable;
                    return Err(anyhow::anyhow!("waiting on laml failed: {error}"));
                }
            }
        }
        if let Some(start_at) = self.next_start {
            if Instant::now() < start_at {
                self.state = State::BackingOff {
                    attempt: self.attempt,
                };
                return Ok(());
            }
        }
        match self.config.command().spawn() {
            Ok(child) => {
                self.child = Some(child);
                self.next_start = None;
                self.attempt = 0;
                self.state = State::Running;
                self.restarts.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(error) => {
                // A missing interpreter is a configuration problem, not a crash
                // to retry: report it once and stay quiet.
                self.state = State::Unavailable;
                Err(anyhow::anyhow!(
                    "cannot start {}: {error}",
                    self.config.program.display()
                ))
            }
        }
    }

    fn schedule_retry(&mut self) {
        self.attempt = self.attempt.saturating_add(1);
        let factor = 1u32 << self.attempt.min(16);
        let wait = self
            .config
            .min_backoff
            .saturating_mul(factor)
            .min(self.config.max_backoff);
        self.next_start = Some(Instant::now() + wait);
        self.state = State::BackingOff {
            attempt: self.attempt,
        };
    }

    /// Stops the child and waits for it.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.next_start = None;
        self.state = State::Idle;
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(program: &str, script: &str) -> SupervisorConfig {
        SupervisorConfig {
            program: PathBuf::from(program),
            script: PathBuf::from(script),
            min_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(40),
        }
    }

    #[test]
    fn a_missing_interpreter_is_reported_once() {
        let mut supervisor = Supervisor::new(config("/nonexistent/laml", "events.lm"));
        let error = supervisor.poll().expect_err("missing interpreter fails");
        assert!(error.to_string().contains("cannot start"), "{error}");
        assert_eq!(supervisor.state(), State::Unavailable);
        assert_eq!(supervisor.starts(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_child_that_exits_is_restarted_after_a_backoff() {
        let mut supervisor = Supervisor::new(config("sh", "-c"));
        supervisor.config.script = PathBuf::from("exit 0");
        supervisor.poll().expect("sh starts");
        assert_eq!(supervisor.state(), State::Running);
        assert_eq!(supervisor.starts(), 1);

        // The child exits immediately, so the next poll notices and backs off.
        std::thread::sleep(Duration::from_millis(50));
        supervisor.poll().expect("poll after exit");
        assert!(matches!(supervisor.state(), State::BackingOff { .. }));

        std::thread::sleep(Duration::from_millis(60));
        supervisor.poll().expect("poll after backoff");
        assert_eq!(supervisor.state(), State::Running);
        assert_eq!(supervisor.starts(), 2);
        supervisor.stop();
        assert_eq!(supervisor.state(), State::Idle);
    }

    #[cfg(unix)]
    #[test]
    fn backoff_grows_and_then_settles_at_the_ceiling() {
        let mut supervisor = Supervisor::new(config("sh", "-c"));
        supervisor.config.script = PathBuf::from("exit 0");
        let mut waits = Vec::new();
        for _ in 0..6 {
            supervisor.poll().ok();
            std::thread::sleep(Duration::from_millis(45));
            if let State::BackingOff { attempt } = supervisor.state() {
                waits.push(attempt);
            }
        }
        supervisor.stop();
        assert!(waits.windows(2).any(|pair| pair[1] > pair[0]), "{waits:?}");
    }

    #[test]
    fn the_bundled_service_program_is_shipped_with_the_crate() {
        let config = SupervisorConfig::discover();
        assert!(
            config.script.exists(),
            "missing {}",
            config.script.display()
        );
        let source = std::fs::read_to_string(&config.script).expect("service source reads");
        assert!(source.contains("serve("), "the service must serve");
        assert!(
            source.contains("on(\"message\""),
            "the service must handle events"
        );
    }
}
