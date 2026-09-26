//! The state one native window draws.
//!
//! The window owns three things: the [`App`] model that decides which pane is
//! showing, the terminals and documents those panes actually run, and whatever
//! the event service has published to the rooms the panes watch. Nothing here
//! knows about pixels, so every rule below is testable without a GPU.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use keplr_client::{App, Document, Key, Pane, Terminal, TerminalHost};
use keplr_events::{channel, drain, Event, Service, UnboundedReceiver};
use keplr_term::{Snapshot, TerminalGrid};
use rcus::desktop::Redraw;
use rcus::{InputEvent, Key as RKey, Modifiers};

/// Room the problems pane watches, as the model already declares it.
const DIAGNOSTICS_ROOM: &str = "diagnostics.update";
/// Room the source control pane watches.
const TASKS_ROOM: &str = "task.update";
/// Terminal grid the first pane is opened with, before the window is measured.
const DEFAULT_COLS: usize = 100;
const DEFAULT_ROWS: usize = 28;

/// Repaints the window whenever the shell writes. The pty reader runs on its
/// own thread, so all it can do is ask for a frame; the window thread rebuilds
/// the view when the tick arrives.
struct Painter {
    redraw: Redraw,
}

impl TerminalHost for Painter {
    fn on_frame(&mut self, _grid: &TerminalGrid) {
        self.redraw.request();
    }

    fn on_exit(&mut self) {
        self.redraw.request();
    }
}

/// The state behind one window.
pub struct State {
    /// Which panes are open and which one is showing.
    pub client: App,
    /// Live shells, keyed by session so a pane switch keeps the process.
    terminals: HashMap<String, Terminal>,
    /// Open editors, keyed by path, so a tab switch keeps the cursor.
    documents: HashMap<PathBuf, Document>,
    /// `path: count` lines from the diagnostics room.
    diagnostics: Vec<String>,
    /// `id state` lines from the task room.
    tasks: Vec<String>,
    /// The last thing that went wrong, shown in the status bar.
    pub status: String,
    shell: String,
    /// The window's redraw handle. It only exists once the rcus app is built,
    /// which is after the first view, so terminals are spawned later and this
    /// slot is filled in before any key can arrive.
    redraw: Arc<Mutex<Option<Redraw>>>,
    /// Frames from the rooms this window watches.
    events: UnboundedReceiver<String>,
    /// Subscription ids, released when the window closes.
    _watch: Vec<u64>,
}

impl State {
    /// Builds the state for a window over `root`, starting on the problems pane
    /// the model opens with.
    pub fn new(
        root: impl Into<PathBuf>,
        shell: String,
        redraw: Arc<Mutex<Option<Redraw>>>,
    ) -> Self {
        let client = App::new(root, Service::new());
        let (sender, events) = channel();
        let _watch = vec![
            client.events().subscribe(DIAGNOSTICS_ROOM, sender.clone()),
            client.events().subscribe(TASKS_ROOM, sender),
        ];
        Self {
            client,
            terminals: HashMap::new(),
            documents: HashMap::new(),
            diagnostics: Vec::new(),
            tasks: Vec::new(),
            status: "ready".to_string(),
            shell,
            redraw,
            events,
            _watch,
        }
    }

    /// Hands the window the handle that asks for a frame, which only exists
    /// after the rcus app is built. Terminals started before this would have
    /// nowhere to draw.
    pub fn install_redraw(&mut self, redraw: Redraw) {
        if let Ok(mut slot) = self.redraw.lock() {
            *slot = Some(redraw);
        }
    }

    /// The current frame of a terminal pane, or `None` if it has no shell yet.
    pub fn terminal(&self, session: &str) -> Option<Snapshot> {
        self.terminals
            .get(session)
            .map(|terminal| terminal.snapshot())
    }

    /// The document an editor pane is showing, loaded on first use.
    pub fn document(&mut self, path: &Path) -> Option<&Document> {
        if !self.documents.contains_key(path) {
            let document = Document::open(path).ok()?;
            self.documents.insert(path.to_path_buf(), document);
        }
        self.documents.get(path)
    }

    /// Problems, as the diagnostics room last reported them.
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Background tasks, as the task room last reported them.
    pub fn tasks(&self) -> &[String] {
        &self.tasks
    }

    /// Drains the event rooms into the panes that watch them, returning true if
    /// anything changed so the caller can skip a repaint.
    pub fn drain_events(&mut self) -> bool {
        // The receiver and the panes are separate fields, so the drain closure
        // can hold one while it writes the other.
        let Self {
            events,
            diagnostics,
            tasks,
            ..
        } = self;
        drain(events, |frame| apply_frame(frame, diagnostics, tasks))
    }

    /// Applies one event frame, ignoring anything this window does not watch.
    pub fn apply_frame(&mut self, frame: &str) -> bool {
        apply_frame(frame, &mut self.diagnostics, &mut self.tasks)
    }

    /// Applies one input event, returning true if it was consumed. Anything not
    /// consumed here is a binding the window does not have yet.
    pub fn key(&mut self, event: &InputEvent) -> bool {
        let InputEvent::KeyDown { key, modifiers } = event else {
            return false;
        };
        let modifiers = *modifiers;
        if self.shortcut(key, modifiers) {
            return true;
        }
        match self.client.active().map(|tab| tab.pane.clone()) {
            Some(Pane::Terminal { session }) => self.terminal_key(&session, key, modifiers),
            Some(Pane::Editor { path, .. }) => self.editor_key(&path, key, modifiers),
            Some(Pane::Problems) | Some(Pane::SourceControl) | None => false,
        }
    }

    /// Window-level shortcuts, which win even while a pane has focus.
    fn shortcut(&mut self, key: &RKey, modifiers: Modifiers) -> bool {
        if modifiers.control && modifiers.alt {
            match character(key) {
                Some('t') => return self.open_terminal(),
                Some('e') => return self.open_editor(),
                Some('p') => {
                    self.client.open(Pane::Problems);
                }
                Some('s') => {
                    self.client.open(Pane::SourceControl);
                }
                _ => return false,
            }
            self.status = "ready".to_string();
            return true;
        }
        if modifiers.control {
            if matches!(key, RKey::Tab) {
                self.client.cycle(!modifiers.shift);
                return true;
            }
            if character(key) == Some('w') {
                let index = self.client.active_index();
                self.client.close(index);
                return true;
            }
        }
        false
    }

    /// Opens the terminal pane, starting the shell the first time.
    pub fn open_terminal(&mut self) -> bool {
        let session = "terminal".to_string();
        if !self.terminals.contains_key(&session) {
            let redraw = self.redraw.lock().ok().and_then(|slot| slot.clone());
            let Some(redraw) = redraw else {
                self.status = "the window is not ready for a shell yet".to_string();
                return false;
            };
            let cwd = self.client.root().to_path_buf();
            let shell = self.shell.clone();
            match Terminal::spawn(&shell, &cwd, DEFAULT_COLS, DEFAULT_ROWS, Painter { redraw }) {
                Ok(terminal) => {
                    self.terminals.insert(session.clone(), terminal);
                }
                Err(error) => {
                    self.status = format!("{shell} did not start: {error}");
                    return false;
                }
            }
        }
        self.status = "ready".to_string();
        self.client.open(Pane::Terminal { session });
        true
    }

    /// Opens the editor pane on a file this window already has open, or on the
    /// first source file under the root, so the shortcut always goes somewhere.
    pub fn open_editor(&mut self) -> bool {
        let path = self
            .documents
            .keys()
            .next()
            .cloned()
            .or_else(|| first_source_file(self.client.root()));
        let Some(path) = path else {
            self.status = format!("no source file under {}", self.client.root().display());
            return false;
        };
        if self.document(&path).is_none() {
            self.status = format!("{} could not be read", path.display());
            return false;
        }
        self.status = "ready".to_string();
        self.client.open(Pane::Editor {
            path,
            cursor: 0,
            top_line: 0,
        });
        true
    }

    /// Sends one key to a terminal pane.
    fn terminal_key(&mut self, session: &str, key: &RKey, modifiers: Modifiers) -> bool {
        let Some(terminal) = self.terminals.get(session) else {
            return false;
        };
        let Some(key) = translate_key(key, modifiers) else {
            return false;
        };
        if let Err(error) = terminal.key(key) {
            self.status = format!("the shell stopped accepting keys: {error}");
        }
        true
    }

    /// Applies one key to an editor pane.
    fn editor_key(&mut self, path: &Path, key: &RKey, modifiers: Modifiers) -> bool {
        if self.document(path).is_none() {
            return false;
        }
        if modifiers.control && character(key) == Some('s') {
            let document = self.documents.get_mut(path).expect("loaded above");
            return match document.save() {
                Ok(()) => {
                    self.status = format!("saved {}", document.path().display());
                    true
                }
                Err(error) => {
                    self.status = format!("save failed: {error}");
                    true
                }
            };
        }
        let Some(document) = self.documents.get_mut(path) else {
            return false;
        };
        let dirty_before = document.is_dirty();
        match key {
            RKey::Character(text) if !modifiers.control && !modifiers.meta => {
                document.insert(text);
            }
            RKey::Backspace => document.backspace(),
            RKey::ArrowLeft => document.left(),
            RKey::ArrowRight => document.right(),
            RKey::PageUp => document.scroll(-10),
            RKey::PageDown => document.scroll(10),
            RKey::Home => document.goto(document.cursor_line(), 0),
            RKey::End => {
                let line = document.cursor_line();
                let end = document.text().len();
                document.goto(line, end);
            }
            RKey::Enter => document.insert("\n"),
            _ => return false,
        }
        document.scroll_to_cursor();
        if document.is_dirty() && !dirty_before {
            self.status = "modified".to_string();
        }
        true
    }

    /// Resizes the showing terminal to the pane it was measured into.
    pub fn resize_terminal(&mut self, cols: usize, rows: usize) {
        let Some(Pane::Terminal { session }) = self.client.active().map(|tab| &tab.pane) else {
            return;
        };
        if let Some(terminal) = self.terminals.get_mut(session) {
            terminal.resize(cols, rows);
        }
    }
}

/// Applies one event frame to the panes that watch rooms, and reports whether
/// anything moved. A free function so a drain can borrow the receiver and the
/// panes at the same time.
fn apply_frame(frame: &str, diagnostics: &mut Vec<String>, tasks: &mut Vec<String>) -> bool {
    let Ok(event) = serde_json::from_str::<Event>(frame) else {
        return false;
    };
    match event {
        Event::DiagnosticsUpdate { path, count } => {
            diagnostics.retain(|line| !line.starts_with(&path));
            if count > 0 {
                diagnostics.push(format!("{path}: {count}"));
            }
            diagnostics.sort();
            true
        }
        Event::TaskUpdate { id, state, .. } => {
            tasks.retain(|line| !line.starts_with(&id));
            tasks.push(format!("{id} {}", format!("{state:?}").to_lowercase()));
            tasks.sort();
            true
        }
        _ => false,
    }
}

/// The character a key carries, if it is one.
fn character(key: &RKey) -> Option<char> {
    match key {
        RKey::Character(text) => text.chars().next(),
        _ => None,
    }
}

/// Translates a window key into a terminal key, or `None` if the terminal has
/// no meaning for it. Control characters are sent as themselves, so Ctrl-C is
/// the interrupt the shell expects.
fn translate_key(key: &RKey, modifiers: Modifiers) -> Option<Key> {
    if modifiers.meta {
        return None;
    }
    if modifiers.control {
        return match character(key) {
            Some('c') => Some(Key::Interrupt),
            Some(ch) if ch.is_ascii_alphabetic() => {
                let code = (ch.to_ascii_lowercase() as u8) - 96;
                Some(Key::Char(code as char))
            }
            _ => None,
        };
    }
    match key {
        RKey::Character(text) => text.chars().next().map(Key::Char),
        RKey::Enter => Some(Key::Enter),
        RKey::Tab => Some(Key::Tab),
        RKey::Backspace => Some(Key::Backspace),
        RKey::Escape => Some(Key::Escape),
        RKey::ArrowUp => Some(Key::Up),
        RKey::ArrowDown => Some(Key::Down),
        RKey::ArrowLeft => Some(Key::Left),
        RKey::ArrowRight => Some(Key::Right),
        RKey::Home => Some(Key::Home),
        RKey::End => Some(Key::End),
        RKey::PageUp => Some(Key::PageUp),
        RKey::PageDown => Some(Key::PageDown),
        RKey::Delete => Some(Key::Delete),
    }
}

/// The first source file under `root`, in a stable order, so the editor
/// shortcut opens the same file twice.
fn first_source_file(root: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| {
            matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("rs" | "js" | "mjs" | "lm" | "toml" | "html")
            )
        })
        .filter(|path| !path.to_string_lossy().contains("/target/"))
        .collect();
    found.sort();
    found.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcus::Key as RKey;

    /// A state with a redraw handle, which is what a live window has. The
    /// handle is a stub: there is no event loop in a test, and the pty reader
    /// only ever asks it to draw.
    fn state(root: &Path) -> State {
        State::new(
            root,
            "sh".to_string(),
            Arc::new(Mutex::new(Some(Redraw::default()))),
        )
    }

    /// A state with no window behind it yet.
    fn headless(root: &Path) -> State {
        State::new(root, "sh".to_string(), Arc::new(Mutex::new(None)))
    }

    fn down(key: RKey) -> InputEvent {
        InputEvent::KeyDown {
            key,
            modifiers: Modifiers::default(),
        }
    }

    fn chord(key: RKey, control: bool, alt: bool) -> InputEvent {
        InputEvent::KeyDown {
            key,
            modifiers: Modifiers {
                control,
                alt,
                shift: false,
                meta: false,
            },
        }
    }

    #[test]
    fn a_terminal_pane_starts_a_shell_only_once() {
        let mut state = state(std::path::Path::new("/tmp"));
        assert!(state.open_terminal(), "the first open starts a shell");
        let frame = state.terminal("terminal").expect("the pane has a grid");
        assert_eq!(frame.cols, 100, "the shell was started at the pane's width");
        assert_eq!(frame.rows, 28);
        // A second open focuses the same tab rather than forking a new shell.
        let tabs = state.client.tabs().len();
        assert!(state.open_terminal());
        assert_eq!(state.client.tabs().len(), tabs);
    }

    #[test]
    fn a_window_asking_for_a_shell_before_it_is_ready_says_so() {
        let mut state = headless(std::path::Path::new("/tmp"));
        assert!(
            !state.open_terminal(),
            "no redraw handle means no window to draw into"
        );
        assert!(state.status.contains("not ready"), "{}", state.status);
    }

    #[test]
    fn control_characters_reach_the_shell_as_control_characters() {
        assert_eq!(
            translate_key(&RKey::Character("c".into()), ctrl(true, false, false)),
            Some(Key::Interrupt)
        );
        assert_eq!(
            translate_key(&RKey::Character("d".into()), ctrl(true, false, false)),
            Some(Key::Char('\u{4}'))
        );
        assert_eq!(
            translate_key(&RKey::Enter, Modifiers::default()),
            Some(Key::Enter)
        );
        assert_eq!(
            translate_key(&RKey::Character("a".into()), Modifiers::default()),
            Some(Key::Char('a'))
        );
    }

    #[test]
    fn a_key_the_terminal_has_no_meaning_for_is_reported_as_unhandled() {
        assert_eq!(
            translate_key(&RKey::Character("z".into()), ctrl(false, false, true)),
            None
        );
        assert_eq!(
            translate_key(&RKey::Character("z".into()), ctrl(true, true, false)),
            None
        );
    }

    #[test]
    fn a_diagnostics_frame_fills_the_problems_pane() {
        let mut state = state(std::path::Path::new("/tmp"));
        assert!(state.diagnostics().is_empty());
        state.apply_frame(r#"{"kind":"diagnosticsUpdate","path":"src/lib.rs","count":3}"#);
        assert_eq!(
            state.diagnostics().to_vec(),
            vec!["src/lib.rs: 3".to_string()]
        );
        // A later report for the same file replaces the old count instead of
        // stacking a second line.
        state.apply_frame(r#"{"kind":"diagnosticsUpdate","path":"src/lib.rs","count":0}"#);
        assert!(state.diagnostics().is_empty(), "{:?}", state.diagnostics());
    }

    #[test]
    fn a_task_frame_fills_the_source_control_pane() {
        let mut state = state(std::path::Path::new("/tmp"));
        state.apply_frame(r#"{"kind":"taskUpdate","id":"build","state":"passed"}"#);
        assert_eq!(state.tasks().to_vec(), vec!["build passed".to_string()]);
        state.apply_frame(
            r#"{"kind":"taskUpdate","id":"build","state":"failed","detail":"2 errors"}"#,
        );
        assert_eq!(state.tasks().to_vec(), vec!["build failed".to_string()]);
    }

    #[test]
    fn a_frame_this_window_does_not_watch_changes_nothing() {
        let mut state = state(std::path::Path::new("/tmp"));
        assert!(!state.apply_frame(r#"{"kind":"ping","id":7}"#));
        assert!(!state.apply_frame("not json at all"));
    }

    #[test]
    fn control_tab_cycles_tabs_and_control_w_closes_one() {
        let mut state = state(std::path::Path::new("/tmp"));
        let before = state.client.active_index();
        assert!(state.key(&chord(RKey::Tab, true, false)));
        assert_ne!(state.client.active_index(), before);
        let tabs = state.client.tabs().len();
        assert!(state.key(&chord(RKey::Character("w".into()), true, false)));
        assert_eq!(state.client.tabs().len(), tabs - 1);
    }

    #[test]
    fn typing_into_a_problems_pane_is_not_a_binding() {
        let mut state = state(std::path::Path::new("/tmp"));
        assert!(!state.key(&down(RKey::Character("x".into()))));
    }

    #[test]
    fn an_editor_pane_edits_its_document() {
        let mut state = state(std::path::Path::new("/tmp"));
        let path = std::path::Path::new("/tmp/keplr-native-editor-test.txt");
        std::fs::write(path, "one\ntwo\n").expect("fixture written");
        state.documents.insert(
            path.to_path_buf(),
            Document::open(path).expect("fixture opens"),
        );
        state.client.open(Pane::Editor {
            path: path.to_path_buf(),
            cursor: 0,
            top_line: 0,
        });
        assert!(state.key(&down(RKey::Character("x".into()))));
        assert!(state.key(&down(RKey::Backspace)));
        let document = state.documents.get(path).expect("still open");
        assert_eq!(document.text(), "one\ntwo\n", "the backspace undid it");
        assert_eq!(document.cursor(), 1, "the cursor went back too");
        std::fs::remove_file(path).ok();
    }

    fn ctrl(control: bool, shift: bool, meta: bool) -> Modifiers {
        Modifiers {
            control,
            alt: false,
            shift,
            meta,
        }
    }
}
