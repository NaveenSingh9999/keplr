//! A shell on a pseudo terminal, feeding a [`TerminalGrid`] on its own thread.
//!
//! The browser path does the same thing over a websocket; putting it here means
//! the native client gets identical behavior without an HTTP hop in between.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use portable_pty::{Child, MasterPty};

use crate::TerminalGrid;

/// What a session reports back to a renderer.
pub trait SessionSink: Send {
    /// Called after output has been applied to the grid.
    fn on_output(&mut self, grid: &TerminalGrid);
    /// Called once the shell is gone.
    fn on_exit(&mut self);
}

/// A live shell. Dropping it closes the pty, which ends the shell.
pub struct PtySession {
    /// Kept first so it drops before the writer, closing the pty and letting the
    /// reader thread see end of file.
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    grid: Arc<Mutex<TerminalGrid>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    running: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

/// Spawns `shell` in `cwd` at `cols`x`rows`, pumping output into `grid`.
///
/// The reader thread owns the pty reader; writes from the UI thread go through a
/// shared handle so input never blocks behind the shell's own output.
pub fn spawn<S: SessionSink + 'static>(
    shell: &str,
    cwd: &Path,
    cols: usize,
    rows: usize,
    grid: Arc<Mutex<TerminalGrid>>,
    mut sink: S,
) -> anyhow::Result<PtySession> {
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system.openpty(portable_pty::PtySize {
        rows: rows.max(1) as u16,
        cols: cols.max(1) as u16,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut command = portable_pty::CommandBuilder::new(shell);
    command.cwd(cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    let child = pair.slave.spawn_command(command)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let raw_writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = Arc::clone(&running);
    let thread_grid = Arc::clone(&grid);

    let reader_thread = std::thread::Builder::new()
        .name("keplr-pty".into())
        .spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                if let Ok(mut grid) = thread_grid.lock() {
                    grid.advance(&buffer[..count]);
                    sink.on_output(&grid);
                }
            }
            sink.on_exit();
            thread_running.store(false, Ordering::SeqCst);
        })?;

    Ok(PtySession {
        master: pair.master,
        child,
        grid,
        writer: raw_writer,
        running,
        reader: Some(reader_thread),
    })
}

impl PtySession {
    /// The grid this session writes into.
    pub fn grid(&self) -> Arc<Mutex<TerminalGrid>> {
        Arc::clone(&self.grid)
    }

    /// Sends input to the shell.
    pub fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("pty writer poisoned"))?;
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    /// Resizes the grid. The pty itself is sized by the caller that owns the
    /// pty handle, which is the one place that can reach it safely.
    pub fn resize_grid(&self, cols: usize, rows: usize) {
        if let Ok(mut grid) = self.grid.lock() {
            grid.resize(cols, rows);
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// The shell's exit status once it has exited.
    pub fn exit_status(&mut self) -> Option<i32> {
        self.child
            .try_wait()
            .ok()
            .flatten()
            .and_then(|status| status.exit_code())
    }

    /// Asks the shell to exit, then waits briefly for the reader to finish.
    pub fn shutdown(mut self) {
        let _ = self.write(b"exit\n");
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        // The reader thread ends on its own once the master pty closes; joining
        // here would block the UI thread on a shell that ignores the pty.
        self.reader.take();
    }
}
