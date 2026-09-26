//! A terminal a pane owns, driven by a real shell on a pty.
//!
//! The grid and the pty come from `keplr-term`, which the browser path also
//! uses, so both clients see the same cells, the same colors, and the same
//! scrollback.

use std::path::Path;
use std::sync::{Arc, Mutex};

use keplr_term::pty::{self, PtySession, SessionSink};
use keplr_term::{Cell, Snapshot, TerminalGrid};

/// Repaints a pane when the shell produces output.
pub trait TerminalHost: Send {
    /// Called after output has been applied to the grid.
    fn on_frame(&mut self, grid: &TerminalGrid);
    /// Called once the shell is gone.
    fn on_exit(&mut self);
}

struct HostSink<H: TerminalHost + 'static> {
    grid: Arc<Mutex<TerminalGrid>>,
    host: Arc<Mutex<H>>,
}

impl<H: TerminalHost + 'static> SessionSink for HostSink<H> {
    fn on_output(&mut self, grid: &TerminalGrid) {
        let _ = &self.grid;
        if let Ok(mut host) = self.host.lock() {
            host.on_frame(grid);
        }
    }

    fn on_exit(&mut self) {
        if let Ok(mut host) = self.host.lock() {
            host.on_exit();
        }
    }
}

/// One shell, its grid, and the state a pane needs to draw it.
pub struct Terminal {
    session: PtySession,
    grid: Arc<Mutex<TerminalGrid>>,
    cols: usize,
    rows: usize,
}

impl Terminal {
    /// Starts `shell` in `cwd` and hands every frame to `host`.
    pub fn spawn<H: TerminalHost + 'static>(
        shell: &str,
        cwd: &Path,
        cols: usize,
        rows: usize,
        host: H,
    ) -> anyhow::Result<Self> {
        let grid = Arc::new(Mutex::new(TerminalGrid::new(cols, rows)));
        let sink = HostSink {
            grid: Arc::clone(&grid),
            host: Arc::new(Mutex::new(host)),
        };
        let session = pty::spawn(shell, cwd, cols, rows, grid.clone(), sink)?;
        Ok(Self {
            session,
            grid,
            cols: cols.max(1),
            rows: rows.max(1),
        })
    }

    pub fn grid(&self) -> Arc<Mutex<TerminalGrid>> {
        Arc::clone(&self.grid)
    }

    pub fn size(&self) -> (usize, usize) {
        (self.cols, self.rows)
    }

    pub fn is_running(&self) -> bool {
        self.session.is_running()
    }

    /// Sends text as if it were typed, which is what a paste does.
    pub fn paste(&self, text: &str) -> anyhow::Result<()> {
        self.session.write(text.as_bytes())
    }

    /// Sends one key, translating the named keys a terminal understands.
    pub fn key(&self, key: Key) -> anyhow::Result<()> {
        let bytes: &[u8] = match key {
            Key::Char(ch) => {
                let mut buffer = [0u8; 4];
                let encoded = ch.encode_utf8(&mut buffer);
                return self.session.write(encoded.as_bytes());
            }
            Key::Enter => b"\r",
            Key::Tab => b"\t",
            Key::Backspace => b"\x7f",
            Key::Escape => b"\x1b",
            Key::Up => b"\x1b[A",
            Key::Down => b"\x1b[B",
            Key::Right => b"\x1b[C",
            Key::Left => b"\x1b[D",
            Key::Home => b"\x1b[H",
            Key::End => b"\x1b[F",
            Key::PageUp => b"\x1b[5~",
            Key::PageDown => b"\x1b[6~",
            Key::Delete => b"\x1b[3~",
            Key::Interrupt => b"\x03",
        };
        self.session.write(bytes)
    }

    /// Resizes the shell and the grid together.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        self.session.resize(cols, rows);
        self.cols = cols;
        self.rows = rows;
    }

    /// The current frame, ready to draw.
    pub fn snapshot(&self) -> Snapshot {
        match self.grid.lock() {
            Ok(grid) => grid.snapshot(),
            Err(_) => Snapshot {
                cols: self.cols,
                rows: self.rows,
                cursor: keplr_term::Cursor { line: 0, column: 0 },
                show_cursor: true,
                history: Vec::new(),
                cells: vec![
                    vec![
                        Cell {
                            ch: " ".into(),
                            fg: None,
                            bg: None,
                            flags: 0,
                        };
                        self.cols
                    ];
                    self.rows
                ],
            },
        }
    }
}

/// The keys a terminal pane understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    /// Ctrl-C, which stops whatever the shell is running.
    Interrupt,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Counter {
        frames: AtomicUsize,
        exited: AtomicUsize,
    }

    impl TerminalHost for Counter {
        fn on_frame(&mut self, _grid: &TerminalGrid) {
            self.frames.fetch_add(1, Ordering::SeqCst);
        }

        fn on_exit(&mut self) {
            self.exited.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn cell_text(snapshot: &Snapshot, line: usize, column: usize) -> String {
        snapshot.cells[line][column].ch.clone()
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_writes_into_the_grid() {
        let counter = Counter::default();
        let mut terminal =
            Terminal::spawn("sh", Path::new("/tmp"), 40, 6, counter).expect("shell starts");
        for _ in 0..100 {
            if cell_text(&terminal.snapshot(), 0, 0) != " " {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let snapshot = terminal.snapshot();
        assert_eq!(snapshot.cols, 40);
        assert_eq!(snapshot.rows, 6);
        assert!(
            snapshot.cells[0].iter().any(|cell| cell.ch != " "),
            "the shell should have written something: {:?}",
            snapshot.cells[0]
        );
        assert!(terminal.is_running());
        terminal.resize(80, 12);
        assert_eq!(terminal.size(), (80, 12));
        assert_eq!(terminal.snapshot().cols, 80);
    }

    #[cfg(unix)]
    #[test]
    fn a_command_typed_into_the_terminal_shows_up() {
        let counter = Counter::default();
        let terminal =
            Terminal::spawn("sh", Path::new("/tmp"), 60, 8, counter).expect("shell starts");
        terminal.key(Key::Char('e')).expect("writes");
        terminal.paste("cho keplr").expect("writes");
        terminal.key(Key::Enter).expect("writes");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let snapshot = terminal.snapshot();
            let text: String = snapshot
                .cells
                .iter()
                .flat_map(|row| row.iter())
                .map(|cell| cell.ch.as_str())
                .collect();
            if text.contains("keplr") {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the command never ran: {text}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_host_is_told_when_the_shell_exits() {
        struct Exits(Arc<AtomicUsize>);

        impl TerminalHost for Exits {
            fn on_frame(&mut self, _grid: &TerminalGrid) {}
            fn on_exit(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let exits = Arc::new(AtomicUsize::new(0));
        let terminal = Terminal::spawn("sh", Path::new("/tmp"), 20, 4, Exits(Arc::clone(&exits)))
            .expect("shell starts");
        terminal.paste("exit\n").expect("writes");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while exits.load(Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "the host was never told the shell exited"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
