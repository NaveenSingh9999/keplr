//! The state behind the native client, with no renderer attached.
//!
//! The native client is a retained view over this model: a pane is a tab, a tab
//! is a file or a terminal, and the host reduces input into new state and
//! publishes what changed. Keeping the model free of any UI toolkit means it
//! can be tested headlessly, and it is the same model the browser client drives
//! through the HTTP surface.

use std::path::{Path, PathBuf};

use keplr_events::{Command, Event, Service};

/// What a pane shows. There is no card stack: one pane is one thing.
#[derive(Clone, Debug, PartialEq)]
pub enum Pane {
    /// A file, with the cursor and scroll offset the editor uses.
    Editor {
        path: PathBuf,
        cursor: usize,
        top_line: usize,
    },
    /// A terminal, identified so the session survives a pane switch.
    Terminal { session: String },
    /// The problems list.
    Problems,
    /// Source control.
    SourceControl,
}

impl Pane {
    /// The room a window subscribes to while this pane is showing.
    pub fn room(&self) -> &'static str {
        match self {
            Pane::Editor { .. } => "editor",
            Pane::Terminal { .. } => "terminal.frame",
            Pane::Problems => "diagnostics.update",
            Pane::SourceControl => "task.update",
        }
    }

    /// A short label for a tab bar.
    pub fn label(&self) -> String {
        match self {
            Pane::Editor { path, .. } => path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string()),
            Pane::Terminal { session } => session.clone(),
            Pane::Problems => "problems".into(),
            Pane::SourceControl => "source".into(),
        }
    }
}

/// A tab: one pane plus where it was last focused.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub pane: Pane,
    pub focused: bool,
}

/// The left rail, a fixed column of pane kinds rather than movable panes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rail {
    pub width: u16,
}

impl Rail {
    pub const DEFAULT_WIDTH: u16 = 240;

    /// The rail only ever picks which kind of pane opens next; it never holds
    /// cards, so a layout cannot drift into a nested split graph.
    pub fn next_pane(&self, kind: PaneKind) -> Pane {
        match kind {
            PaneKind::Editor => Pane::Problems,
            PaneKind::Terminal => Pane::Terminal {
                session: "terminal".into(),
            },
            PaneKind::Problems => Pane::SourceControl,
            PaneKind::SourceControl => Pane::Problems,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneKind {
    Editor,
    Terminal,
    Problems,
    SourceControl,
}

/// The whole client state.
pub mod document;
pub mod terminal;

pub use document::Document;
pub use terminal::{Key, Terminal, TerminalHost};

pub struct App {
    root: PathBuf,
    rail: Rail,
    tabs: Vec<Tab>,
    active: usize,
    events: Service,
}

impl App {
    pub fn new(root: impl Into<PathBuf>, events: Service) -> Self {
        let mut app = Self {
            root: root.into(),
            rail: Rail {
                width: Rail::DEFAULT_WIDTH,
            },
            tabs: Vec::new(),
            active: 0,
            events,
        };
        app.open(Pane::Problems);
        app
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn events(&self) -> &Service {
        &self.events
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn active(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn rail(&self) -> Rail {
        self.rail
    }

    /// Opens a pane in a tab, or focuses the tab that already shows it.
    pub fn open(&mut self, pane: Pane) -> usize {
        if let Some(index) = self.tabs.iter().position(|tab| tab.pane == pane) {
            self.focus(index);
            return index;
        }
        self.tabs.push(Tab {
            pane,
            focused: true,
        });
        let opened = self.tabs.len() - 1;
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            tab.focused = index == opened;
        }
        self.active = opened;
        opened
    }

    /// Focuses a tab, which is also how a pane becomes the room to subscribe to.
    pub fn focus(&mut self, index: usize) -> bool {
        let count = self.tabs.len();
        if index >= count {
            return false;
        }
        self.active = index;
        for (position, tab) in self.tabs.iter_mut().enumerate() {
            tab.focused = position == index;
        }
        true
    }

    /// Closes a tab, keeping the active index inside the list.
    pub fn close(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }
        let was_active = index == self.active;
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
            return true;
        }
        if was_active {
            self.active = index.min(self.tabs.len() - 1);
            self.tabs[self.active].focused = true;
        }
        true
    }

    /// Cycles to the next tab, which is what a single key does.
    pub fn cycle(&mut self, forward: bool) {
        if self.tabs.is_empty() {
            return;
        }
        let last = self.tabs.len() - 1;
        let next = if forward {
            (self.active + 1).min(last)
        } else {
            self.active.checked_sub(1).unwrap_or(last)
        };
        self.focus(next);
    }

    /// Publishes the current pane kind, so every window watching that room
    /// repaints.
    pub fn publish_pane(&self) -> Vec<Command> {
        let Some(tab) = self.active() else {
            return Vec::new();
        };
        let kind = tab.pane.room();
        self.events.broadcast(
            &Service::room(kind),
            &keplr_events::encode_event(&Event::Watch {
                of: kind.to_string(),
            }),
        );
        vec![Command::Invalidate {
            reason: kind.to_string(),
        }]
    }

    /// The rooms a window should be subscribed to for the current state.
    pub fn rooms(&self) -> Vec<String> {
        let mut rooms = vec!["keplr".to_string()];
        if let Some(tab) = self.active() {
            rooms.push(Service::room(tab.pane.room()));
        }
        rooms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new("/tmp/keplr-client-test", Service::new())
    }

    fn editor(name: &str) -> Pane {
        Pane::Editor {
            path: PathBuf::from(format!("/tmp/{name}")),
            cursor: 0,
            top_line: 0,
        }
    }

    #[test]
    fn a_new_app_starts_on_problems() {
        let app = app();
        assert_eq!(
            app.active().map(|tab| tab.pane.clone()),
            Some(Pane::Problems)
        );
    }

    #[test]
    fn opening_the_same_pane_focuses_its_tab() {
        let mut app = app();
        let first = app.open(editor("a.rs"));
        let second = app.open(editor("b.rs"));
        assert_ne!(first, second);
        let again = app.open(editor("b.rs"));
        assert_eq!(again, second, "an already open pane reuses its tab");
        assert_eq!(app.tabs().len(), 3);
        assert_eq!(app.active_index(), second);
    }

    #[test]
    fn exactly_one_tab_is_focused() {
        let mut app = app();
        app.open(editor("a.rs"));
        app.open(editor("b.rs"));
        app.focus(0);
        let focused: Vec<usize> = app
            .tabs()
            .iter()
            .enumerate()
            .filter(|(_, tab)| tab.focused)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(focused, vec![0]);
    }

    #[test]
    fn closing_the_active_tab_focuses_a_neighbour() {
        let mut app = app();
        let a = app.open(editor("a.rs"));
        let b = app.open(editor("b.rs"));
        assert_eq!(app.active_index(), b);
        app.close(b);
        assert_eq!(app.active_index(), a);
        assert!(app.tabs().get(b).is_none());
    }

    #[test]
    fn closing_the_last_tab_is_allowed() {
        let mut app = app();
        assert!(app.close(0));
        assert!(app.tabs().is_empty());
        assert_eq!(app.active_index(), 0);
    }

    #[test]
    fn focusing_a_missing_tab_is_refused() {
        let mut app = app();
        assert!(!app.focus(9));
    }

    #[test]
    fn cycling_wraps_in_both_directions() {
        let mut app = app();
        app.open(editor("a.rs"));
        app.open(editor("b.rs"));
        // Three tabs: problems, a.rs, b.rs, with b.rs focused.
        assert_eq!(app.active_index(), 2);
        app.cycle(false);
        assert_eq!(app.active_index(), 1);
        app.cycle(false);
        app.cycle(false);
        assert_eq!(app.active_index(), 2, "backwards from the first tab wraps");
        app.cycle(true);
        assert_eq!(app.active_index(), 2, "forwards from the last tab stays");
    }

    #[test]
    fn the_active_pane_decides_the_room() {
        let mut app = app();
        app.open(Pane::Terminal {
            session: "build".into(),
        });
        let rooms = app.rooms();
        assert!(
            rooms.contains(&Service::room("terminal.frame")),
            "{rooms:?}"
        );
        let commands = app.publish_pane();
        assert_eq!(
            commands,
            vec![Command::Invalidate {
                reason: "terminal.frame".into()
            }]
        );
    }

    #[test]
    fn a_pane_label_is_short() {
        assert_eq!(
            Pane::Editor {
                path: PathBuf::from("/a/b/main.rs"),
                cursor: 0,
                top_line: 0
            }
            .label(),
            "main.rs"
        );
        assert_eq!(
            Pane::Terminal {
                session: "build".into()
            }
            .label(),
            "build"
        );
    }

    #[test]
    fn the_rail_picks_the_next_pane_without_holding_cards() {
        let rail = Rail::default();
        assert!(matches!(
            rail.next_pane(PaneKind::Terminal),
            Pane::Terminal { .. }
        ));
        assert_eq!(rail.next_pane(PaneKind::Problems), Pane::SourceControl);
    }
}
