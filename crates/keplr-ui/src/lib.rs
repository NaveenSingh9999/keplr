use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dock {
    Left,
    Right,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub path: PathBuf,
    pub cursor: (usize, usize),
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub root: PathBuf,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub left_visible: bool,
    pub right_visible: bool,
    pub bottom_visible: bool,
    pub palette_open: bool,
    pub palette_query: String,
    pub terminal_lines: Vec<String>,
    pub diagnostics: Vec<String>,
    pub vim_mode: bool,
}

impl UiState {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            tabs: Vec::new(),
            active: 0,
            left_visible: true,
            right_visible: true,
            bottom_visible: true,
            palette_open: false,
            palette_query: String::new(),
            terminal_lines: vec![String::from("keplr ready")],
            diagnostics: Vec::new(),
            vim_mode: false,
        }
    }

    pub fn open_file(&mut self, path: PathBuf) {
        if let Some(idx) = self.tabs.iter().position(|t| t.path == path) {
            self.active = idx;
            return;
        }
        self.tabs.push(Tab {
            path,
            cursor: (1, 1),
        });
        self.active = self.tabs.len() - 1;
    }

    pub fn toggle(&mut self, dock: Dock) {
        match dock {
            Dock::Left => self.left_visible = !self.left_visible,
            Dock::Right => self.right_visible = !self.right_visible,
            Dock::Bottom => self.bottom_visible = !self.bottom_visible,
        }
    }

    pub fn palette_results(&self, limit: usize) -> Vec<PathBuf> {
        let ws = keplr_core::Workspace::new(self.root.clone());
        let entries = ws.walk_files(20_000);
        let paths: Vec<PathBuf> = entries.into_iter().map(|e| e.path).collect();
        if self.palette_query.is_empty() {
            return paths.into_iter().take(limit).collect();
        }
        keplr_core::search::fuzzy_paths(&paths, &self.palette_query, limit)
    }

    pub fn active_editor(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn push_terminal(&mut self, line: String) {
        self.terminal_lines.push(line);
        if self.terminal_lines.len() > 200 {
            let excess = self.terminal_lines.len() - 200;
            self.terminal_lines.drain(0..excess);
        }
    }

    pub fn push_diagnostic(&mut self, line: String) {
        self.diagnostics.push(line);
    }

    pub fn to_scene(&self, width: u16) -> keplr_render::Scene {
        let open: Option<PathBuf> = self.active_editor().map(|t| t.path.clone());
        let palette = if self.palette_open {
            Some(self.palette_query.as_str())
        } else {
            None
        };
        let mut scene = keplr_render::build_scene(
            &self.root,
            open.as_deref(),
            &self.palette_query,
            palette,
            width,
        );
        if !self.left_visible {
            scene.left = keplr_render::Panel {
                title: String::from("project (hidden)"),
                lines: vec![String::from("(hidden)")],
            };
        }
        if !self.right_visible {
            scene.right = keplr_render::Panel {
                title: String::from("outline (hidden)"),
                lines: vec![String::from("(hidden)")],
            };
        }
        if !self.bottom_visible {
            scene.bottom = keplr_render::Panel {
                title: String::from("terminal (hidden)"),
                lines: vec![String::from("(hidden)")],
            };
        } else if let Some(last) = self.terminal_lines.last() {
            if !scene.bottom.lines.iter().any(|l| l == last) {
                scene.bottom.lines = self
                    .terminal_lines
                    .iter()
                    .rev()
                    .take(8)
                    .rev()
                    .cloned()
                    .collect();
            } else {
                let mut merged = self
                    .terminal_lines
                    .iter()
                    .rev()
                    .take(8)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>();
                if !merged.contains(&scene.bottom.lines[0]) {
                    merged.insert(0, scene.bottom.lines[0].clone());
                }
                scene.bottom.lines = merged;
            }
        }
        if !self.diagnostics.is_empty() {
            scene.status.errors = self.diagnostics.len();
        }
        if let Some(tab) = self.active_editor() {
            scene.center.cursor = tab.cursor;
        }
        scene
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    OpenPalette,
    ClosePalette,
    ToggleLeft,
    ToggleRight,
    ToggleBottom,
    NextTab,
    PrevTab,
    Quit,
    Unknown,
}

pub fn key_action(key: &str) -> Action {
    match key.to_ascii_lowercase().as_str() {
        "ctrl+p" | "cmd+p" | "ctrl+shift+p" | "cmd+shift+p" => Action::OpenPalette,
        "escape" | "esc" => Action::ClosePalette,
        "ctrl+b" | "cmd+b" => Action::ToggleLeft,
        "ctrl+alt+r" => Action::ToggleRight,
        "ctrl+j" | "ctrl+`" => Action::ToggleBottom,
        "ctrl+tab" | "cmd+]" => Action::NextTab,
        "ctrl+shift+tab" | "cmd+[" => Action::PrevTab,
        "ctrl+q" | "cmd+q" => Action::Quit,
        _ => Action::Unknown,
    }
}

pub fn branch_name(root: &Path) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                String::from("no-git")
            } else {
                s
            }
        }
        _ => String::from("no-git"),
    }
}
