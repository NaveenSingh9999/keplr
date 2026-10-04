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
use keplr_theme::UserTheme;

use crate::theme::Chrome;
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

/// One row in the sidebar: a glyph and a label.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub glyph: char,
    pub label: String,
}

/// What the activity bar is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarView {
    #[default]
    Files,
    Search,
    Source,
    Outline,
}

impl SidebarView {
    pub fn glyph(self) -> char {
        match self {
            SidebarView::Files => '\u{f07b}',
            SidebarView::Search => '\u{f002}',
            SidebarView::Source => '\u{f418}',
            SidebarView::Outline => '\u{f0ae}',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SidebarView::Files => "files",
            SidebarView::Search => "search",
            SidebarView::Source => "source",
            SidebarView::Outline => "outline",
        }
    }

    /// A name safe to put in a node id.
    pub fn slug(self) -> &'static str {
        self.label()
    }
}

/// Which edge a drag has grabbed, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drag {
    Sidebar,
    Panel,
}

/// How close to an edge a press has to land to grab it, in logical pixels.
/// Wide enough to hit with a finger, which is what a laptop trackpad is not.
const EDGE: f32 = 6.0;

/// The state behind one window.
pub struct State {
    /// Which panes are open and which one is showing.
    pub client: App,
    /// The user's theme, resolved once at startup.
    theme: UserTheme,
    /// Colours parsed from the theme, so the view never re-parses them.
    chrome: Chrome,
    /// Which view the activity bar is showing.
    sidebar_view: SidebarView,
    /// How wide the sidebar is, and how tall the panel is open.
    sidebar_width: f32,
    panel_height: f32,
    /// The window's own size, so a drag can be turned back into a size.
    window: (f32, f32),
    /// What the pointer grabbed, while a drag is in progress.
    drag: Option<Drag>,
    panel_open: bool,
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
        let root: PathBuf = root.into();
        let client = App::new(root.clone(), Service::new());
        let (sender, events) = channel();
        let _watch = vec![
            client.events().subscribe(DIAGNOSTICS_ROOM, sender.clone()),
            client.events().subscribe(TASKS_ROOM, sender),
        ];
        let theme = keplr_theme::resolve(&root, std::env::var("KEPLR_THEME").ok().as_deref());
        let chrome = Chrome::new(&theme);
        let problems = keplr_theme::problems(&root);
        let status = if problems.is_empty() {
            "ready".to_string()
        } else {
            format!("{} theme problem(s): {}", problems.len(), problems[0])
        };
        Self {
            client,
            theme,
            chrome,
            sidebar_view: SidebarView::Files,
            sidebar_width: crate::shell::SIDEBAR_DEFAULT,
            panel_height: crate::shell::PANEL_DEFAULT,
            panel_open: false,
            window: (0.0, 0.0),
            drag: None,
            terminals: HashMap::new(),
            documents: HashMap::new(),
            diagnostics: Vec::new(),
            tasks: Vec::new(),
            status,
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

    /// The theme this window is drawing with.
    pub fn theme(&self) -> &UserTheme {
        &self.theme
    }

    /// Which view the activity bar is showing.
    pub fn sidebar_view(&self) -> SidebarView {
        self.sidebar_view
    }

    /// Switches the sidebar, which is what a click on the activity bar does.
    pub fn set_sidebar_view(&mut self, view: SidebarView) {
        self.sidebar_view = view;
    }

    /// The sidebar's width, clamped to what it can be dragged to.
    pub fn sidebar_width(&self) -> f32 {
        self.sidebar_width
            .clamp(crate::shell::SIDEBAR_MIN, crate::shell::SIDEBAR_MAX)
    }

    /// Drags the sidebar's edge, clamped to its bounds.
    pub fn set_sidebar_width(&mut self, width: f32) {
        self.sidebar_width = width.clamp(crate::shell::SIDEBAR_MIN, crate::shell::SIDEBAR_MAX);
    }

    /// Records a new window size.
    pub fn set_window(&mut self, width: f32, height: f32) {
        self.window = (width, height);
    }

    /// Where the panel's top edge is, which is what the panel drags by.
    fn panel_top(&self) -> f32 {
        let bars = crate::shell::TITLE_BAR_H + crate::shell::STATUS_BAR_H;
        self.window.1 - bars - self.panel_height()
    }

    /// How tall the panel is open.
    pub fn panel_height(&self) -> f32 {
        self.panel_height.max(crate::shell::PANEL_MIN)
    }

    /// Drags the panel's edge.
    pub fn set_panel_height(&mut self, height: f32) {
        self.panel_height = height.max(crate::shell::PANEL_MIN);
    }

    pub fn panel_open(&self) -> bool {
        self.panel_open
    }

    /// Opens or closes the bottom panel, which is what ctrl+j does.
    pub fn set_panel_open(&mut self, open: bool) {
        self.panel_open = open;
    }

    /// The rows the sidebar shows: the workspace, shallow and stable, so the
    /// list is the same every time the window opens.
    pub fn sidebar_rows(&self) -> Vec<Row> {
        let mut rows = vec![Row {
            glyph: '\u{f07c}',
            label: self
                .client
                .root()
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| self.client.root().display().to_string()),
        }];
        let mut found: Vec<(String, Vec<String>)> = Vec::new();
        for entry in walkdir::WalkDir::new(self.client.root())
            .max_depth(2)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str() else {
                continue;
            };
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            let children: Vec<String> = walkdir::WalkDir::new(entry.path())
                .max_depth(1)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|child| child.file_type().is_file())
                .filter_map(|child| child.file_name().to_str().map(str::to_string))
                .collect();
            found.push((name.to_string(), children));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, children) in found.into_iter().take(12) {
            rows.push(Row {
                glyph: '\u{f07b}',
                label: name,
            });
            for child in children.into_iter().take(6) {
                rows.push(Row {
                    glyph: '\u{f15b}',
                    label: child,
                });
            }
        }
        rows
    }

    /// The colours parsed from that theme.
    pub fn chrome(&self) -> Chrome {
        self.chrome.clone()
    }

    /// Re-reads the theme, which is what changing it in a picker does.
    pub fn reload_theme(&mut self) {
        self.theme = keplr_theme::resolve(
            self.client.root(),
            std::env::var("KEPLR_THEME").ok().as_deref(),
        );
        self.chrome = Chrome::new(&self.theme);
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
    /// The window goes through `drain_events`; a snapshot drives this directly,
    /// because it has no host publishing to the rooms.
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
            if character(key) == Some('j') {
                self.panel_open = !self.panel_open;
                return true;
            }
            if character(key) == Some('w') {
                let index = self.client.active_index();
                self.client.close(index);
                return true;
            }
            // The rail has four views and they are numbered, so ctrl+1 through
            // ctrl+4 go straight to one.
            if let Some(digit) = character(key).and_then(|c| c.to_digit(10)) {
                if let Some(view) = crate::shell::ACTIVITY_VIEWS.get(digit as usize - 1) {
                    self.set_sidebar_view(*view);
                    return true;
                }
            }
        }
        false
    }

    /// A pointer event, routed by what is under it.
    ///
    /// `node` is the id of the deepest node under the pointer, so the shell's
    /// own names are the whole hit table: an activity glyph, a tab, a sidebar
    /// row, or one of the two drag edges.
    pub fn pointer(&mut self, event: &InputEvent, node: Option<&str>) -> bool {
        match event {
            InputEvent::PointerDown { x, y, .. } => self.pointer_down(*x, *y, node),
            InputEvent::PointerMove { x, y, .. } => match self.drag {
                Some(Drag::Sidebar) => {
                    self.set_sidebar_width(*x);
                    true
                }
                Some(Drag::Panel) => {
                    self.set_panel_height(self.window.1 - *y);
                    true
                }
                None => false,
            },
            InputEvent::PointerUp { .. } => {
                self.drag = None;
                false
            }
            _ => false,
        }
    }

    /// What the first press of a drag grabbed.
    fn pointer_down(&mut self, x: f32, y: f32, node: Option<&str>) -> bool {
        if let Some(id) = node {
            if id.starts_with("status-bar") {
                self.set_panel_open(!self.panel_open);
                return true;
            }
            if let Some(view) = crate::shell::ACTIVITY_VIEWS
                .iter()
                .find(|view| id.ends_with(view.slug()))
            {
                self.set_sidebar_view(*view);
                return true;
            }
            if let Some(index) = id.strip_prefix("tab-label-").and_then(|n| n.parse().ok()) {
                self.client.focus(index);
                return true;
            }
        }
        // The edges are grab bands rather than drawn handles: a window should not
        // show a grabber to explain that it can be dragged.
        if (x - self.sidebar_width()).abs() <= EDGE {
            self.drag = Some(Drag::Sidebar);
            return true;
        }
        if self.panel_open && (y - self.panel_top()).abs() <= EDGE {
            self.drag = Some(Drag::Panel);
            return true;
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

    /// Sends a line to the terminal pane, so a snapshot or a test has a grid
    /// with something in it rather than an empty prompt.
    pub fn run_in_terminal(&mut self, line: &str) -> bool {
        let Some(Pane::Terminal { session }) = self.client.active().map(|tab| &tab.pane) else {
            return false;
        };
        let Some(terminal) = self.terminals.get(session) else {
            return false;
        };
        terminal
            .paste(line)
            .map_err(|error| {
                self.status = format!("the shell stopped accepting input: {error}");
            })
            .is_ok()
    }

    /// Shows a pane, which is how a snapshot or a test chooses what to draw.
    /// Opening a pane that is already showing just focuses it.
    pub fn open(&mut self, pane: Pane) -> bool {
        self.client.open(pane);
        true
    }

    /// Fills the problems pane, for a snapshot: the window reads this room from
    /// the host, and a snapshot has no host to publish to it.
    pub fn publish_problems(&mut self) {
        for frame in [
            r#"{"kind":"diagnosticsUpdate","path":"crates/keplr-render/src/lib.rs","count":2}"#,
            r#"{"kind":"diagnosticsUpdate","path":"crates/keplr-serve/src/lib.rs","count":1}"#,
        ] {
            self.apply_frame(frame);
        }
    }

    /// Fills the source control pane, the same way.
    pub fn publish_tasks(&mut self) {
        for frame in [
            r#"{"kind":"taskUpdate","id":"build","state":"passed","detail":"keplr-native"}"#,
            r#"{"kind":"taskUpdate","id":"test","state":"running"}"#,
        ] {
            self.apply_frame(frame);
        }
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
    // Alt and Meta chords are refused rather than guessed at: a terminal reads
    // them as its own Meta prefix, and sending Ctrl+Alt+Z as Ctrl+Z would be
    // worse than sending nothing.
    if modifiers.meta || modifiers.alt {
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
            translate_key(
                &RKey::Character("z".into()),
                mods(false, false, false, true)
            ),
            None,
            "meta is not a control character"
        );
        assert_eq!(
            translate_key(
                &RKey::Character("z".into()),
                mods(false, true, false, false)
            ),
            None,
            "alt is a terminal prefix, not a binding here"
        );
        assert_eq!(
            translate_key(&RKey::Character("z".into()), mods(true, false, true, false)),
            Some(Key::Char('\u{1a}')),
            "shift does not change a control chord"
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
        state.client.open(Pane::SourceControl);
        assert_eq!(state.client.active_index(), 1, "the new tab is focused");
        assert!(state.key(&chord(RKey::Tab, true, false)));
        assert_eq!(
            state.client.active_index(),
            1,
            "cycling forwards from the last tab stays there"
        );
        state.client.focus(0);
        assert!(state.key(&chord(RKey::Tab, true, false)));
        assert_eq!(
            state.client.active_index(),
            1,
            "and from the first it moves on"
        );
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
        assert_eq!(document.cursor(), 0, "the cursor went back too");
        std::fs::remove_file(path).ok();
    }

    fn ctrl(control: bool, shift: bool, meta: bool) -> Modifiers {
        mods(control, false, shift, meta)
    }

    fn mods(control: bool, alt: bool, shift: bool, meta: bool) -> Modifiers {
        Modifiers {
            control,
            alt,
            shift,
            meta,
        }
    }
}
