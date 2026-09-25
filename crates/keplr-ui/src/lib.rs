mod workbench;

pub use workbench::{
    LayoutError, PaneAxis, PaneCard, PaneKind, PaneLeaf, PaneNode, PaneTree, WorkbenchLayout,
};

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

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
    pub cursors: Vec<(usize, usize)>,
    pub soft_wrap: bool,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub root: PathBuf,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub left_visible: bool,
    pub right_visible: bool,
    pub bottom_visible: bool,
    pub left_tab: String,
    pub right_tab: String,
    pub bottom_tab: String,
    pub search_query: String,
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_mode: String,
    pub terminal_lines: Vec<String>,
    pub diagnostics: Vec<keplr_lang::Diagnostic>,
    pub tasks: Vec<keplr_render::TaskEntry>,
    pub vim_mode: bool,
    pub workbench: WorkbenchLayout,
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
            left_tab: String::from("project"),
            right_tab: String::from("symbols"),
            bottom_tab: String::from("terminal"),
            search_query: String::new(),
            palette_open: false,
            palette_query: String::new(),
            palette_mode: String::from("files"),
            terminal_lines: vec![String::from("keplr ready")],
            diagnostics: Vec::new(),
            tasks: Vec::new(),
            vim_mode: false,
            workbench: WorkbenchLayout::current_default(),
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
            cursors: vec![(1, 1)],
            soft_wrap: false,
        });
        self.active = self.tabs.len() - 1;
    }

    pub fn workbench_layout(&self) -> &WorkbenchLayout {
        &self.workbench
    }

    pub fn workbench_layout_mut(&mut self) -> &mut WorkbenchLayout {
        &mut self.workbench
    }

    pub fn split_workbench_leaf(
        &mut self,
        leaf_id: &str,
        axis: PaneAxis,
        card: PaneCard,
    ) -> Result<String, LayoutError> {
        self.workbench.tree.split_leaf(leaf_id, axis, card)
    }

    pub fn move_workbench_card(&mut self, card_id: &str, leaf_id: &str) -> Result<(), LayoutError> {
        self.workbench.tree.move_card(card_id, leaf_id)
    }

    pub fn focus_workbench_leaf(&mut self, leaf_id: &str) -> Result<(), LayoutError> {
        self.workbench.focus(leaf_id)
    }

    pub fn set_workbench_leaf_visible(
        &mut self,
        leaf_id: &str,
        visible: bool,
    ) -> Result<(), LayoutError> {
        self.workbench.tree.set_leaf_visible(leaf_id, visible)
    }

    pub fn toggle(&mut self, dock: Dock) {
        match dock {
            Dock::Left => self.left_visible = !self.left_visible,
            Dock::Right => self.right_visible = !self.right_visible,
            Dock::Bottom => self.bottom_visible = !self.bottom_visible,
        }
    }

    pub fn set_left_tab(&mut self, tab: &str) {
        if ["project", "outline", "search"].contains(&tab) {
            self.left_tab = tab.to_string();
        }
    }

    pub fn set_right_tab(&mut self, tab: &str) {
        if ["symbols"].contains(&tab) {
            self.right_tab = tab.to_string();
        }
    }

    pub fn set_bottom_tab(&mut self, tab: &str) {
        if ["terminal", "diagnostics", "tasks"].contains(&tab) {
            self.bottom_tab = tab.to_string();
        }
    }

    pub fn set_palette_mode(&mut self, mode: &str) {
        if ["files", "commands"].contains(&mode) {
            self.palette_mode = mode.to_string();
        }
    }

    pub fn add_cursor(&mut self, line: usize, col: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if !tab.cursors.contains(&(line, col)) {
                tab.cursors.push((line, col));
            }
        }
    }

    pub fn clear_extra_cursors(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.cursors.truncate(1);
            if let Some(first) = tab.cursors.first().cloned() {
                tab.cursor = first;
            }
        }
    }

    pub fn set_soft_wrap(&mut self, wrap: bool) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.soft_wrap = wrap;
        }
    }

    pub fn palette_commands(&self, limit: usize) -> Vec<String> {
        keplr_render::filter_commands(&self.palette_query, limit)
    }

    /// Files mode needs a workspace walk, which has no filesystem on
    /// wasm and yields empty; commands mode is pure and works everywhere.
    pub fn palette_results(&self, limit: usize) -> Vec<String> {
        if self.palette_mode == "commands" {
            return self.palette_commands(limit);
        }
        let ws = keplr_core::Workspace::new(self.root.clone());
        let entries = ws.walk_files(20_000);
        let rel = |p: &PathBuf| {
            p.strip_prefix(&self.root)
                .unwrap_or(p)
                .to_string_lossy()
                .to_string()
        };
        if self.palette_query.is_empty() {
            return entries
                .into_iter()
                .take(limit)
                .map(|e| rel(&e.path))
                .collect();
        }
        let paths: Vec<PathBuf> = entries.into_iter().map(|e| e.path).collect();
        keplr_core::search::fuzzy_paths(&paths, &self.palette_query, limit)
            .into_iter()
            .map(|p| rel(&p))
            .collect()
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

    pub fn push_diagnostic(&mut self, diag: keplr_lang::Diagnostic) {
        self.diagnostics.push(diag);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_tasks(
        &mut self,
        tasks: &BTreeMap<String, keplr_build::TaskDef>,
        workdir: &Path,
        targets: &[String],
        jobs: usize,
        force: bool,
    ) -> anyhow::Result<()> {
        self.bottom_tab = String::from("tasks");
        let reports = keplr_build::run_graph_settled(tasks, workdir, targets, jobs, force)?;
        self.tasks.clear();
        for r in &reports {
            let state = if r.failed {
                "failed"
            } else if r.cancelled {
                "cancelled"
            } else if r.skipped {
                "skipped"
            } else {
                "ok"
            };
            let lines: Vec<&str> = r.output.lines().collect();
            let tail: String = lines
                .iter()
                .skip(lines.len().saturating_sub(5))
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let mut error_file = None;
            let mut error_line = None;
            if r.failed {
                if let Some((f, l)) =
                    r.output.lines().filter_map(parse_file_line).next()
                {
                    error_file = Some(f.clone());
                    error_line = Some(l);
                    self.diagnostics.push(keplr_lang::Diagnostic {
                        path: f,
                        line: l,
                        col: 1,
                        message: tail.clone(),
                        severity: String::from("error"),
                    });
                }
            }
            self.tasks.push(keplr_render::TaskEntry {
                name: r.task.clone(),
                state: state.to_string(),
                output_tail: tail,
                error_file,
                error_line,
            });
            self.push_terminal(format!("=== {} ({state}) ===", r.task));
            for tl in lines.iter().skip(lines.len().saturating_sub(5)) {
                self.push_terminal(tl.to_string());
            }
        }
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn to_scene(&self, width: u16) -> keplr_render::Scene {
        let open: Option<PathBuf> = self.active_editor().map(|t| t.path.clone());
        let search = if self.search_query.is_empty() {
            None
        } else {
            Some(self.search_query.as_str())
        };
        let palette = if self.palette_open {
            Some(self.palette_query.as_str())
        } else {
            None
        };
        let spec = keplr_render::SceneSpec {
            root: &self.root,
            open_file: open.as_deref(),
            query: self.palette_query.as_str(),
            palette_query: palette,
            palette_mode: self.palette_mode.as_str(),
            search_query: search,
            left_tab: self.left_tab.as_str(),
            right_tab: self.right_tab.as_str(),
            bottom_tab: self.bottom_tab.as_str(),
            width,
        };
        let mut scene = keplr_render::build_scene(&spec);
        let mut workbench = self.workbench.clone();
        let _ = workbench
            .tree
            .set_leaf_visible("left", self.left_visible);
        let _ = workbench
            .tree
            .set_leaf_visible("right", self.right_visible);
        let _ = workbench
            .tree
            .set_leaf_visible("bottom", self.bottom_visible);
        scene.layout = serde_json::to_value(&workbench).ok();
        if !self.left_visible {
            scene.left.lines = vec![String::from("(hidden)")];
        }
        if !self.right_visible {
            scene.right.lines = vec![String::from("(hidden)")];
        }
        scene.bottom.tasks = self.tasks.clone();
        if !self.bottom_visible {
            scene.bottom.lines = vec![String::from("(hidden)")];
        } else if self.bottom_tab == "diagnostics" {
            scene.bottom.lines = if self.diagnostics.is_empty() {
                vec![String::from("(no diagnostics)")]
            } else {
                self.diagnostics
                    .iter()
                    .take(8)
                    .map(|d| {
                        format!(
                            "{}:{}:{} [{}] {}",
                            d.path,
                            d.line,
                            d.col,
                            d.severity,
                            d.message.chars().take(80).collect::<String>()
                        )
                    })
                    .collect()
            };
        } else if self.bottom_tab == "tasks" {
            scene.bottom.lines = if self.tasks.is_empty() {
                vec![String::from("(no tasks)")]
            } else {
                self.tasks
                    .iter()
                    .take(8)
                    .map(|t| {
                        let loc = match (&t.error_file, t.error_line) {
                            (Some(f), Some(l)) => format!(" {f}:{l}"),
                            _ => String::new(),
                        };
                        format!("[{}] {}{}", t.state, t.name, loc)
                    })
                    .collect()
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
        for d in &self.diagnostics {
            if d.severity == "error"
                && !scene.center.squiggles.iter().any(|s| {
                    s.line == d.line && s.col == d.col && s.message == d.message
                })
            {
                scene.center.squiggles.push(keplr_render::Squiggle {
                    line: d.line,
                    col: d.col,
                    len: 1,
                    message: d.message.clone(),
                    severity: d.severity.clone(),
                });
            }
        }
        if let Some(tab) = self.active_editor() {
            scene.center.cursor = tab.cursor;
            scene.center.cursors = tab.cursors.clone();
            scene.center.soft_wrap = tab.soft_wrap;
        }
        scene
    }
}

fn parse_file_line(line: &str) -> Option<(String, u64)> {
    const EXTS: &[&str] = &[
        ".tsx", ".jsx", ".cpp", ".hpp", ".rs", ".lm", ".ts", ".js", ".go", ".cc", ".h",
    ];
    for ext in EXTS {
        let mut start = 0;
        while let Some(i) = line[start..].find(ext) {
            let end = start + i + ext.len();
            if let Some(rest) = line[end..].strip_prefix(':') {
                let digits: String =
                    rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse::<u64>() {
                    if n > 0 {
                        let bytes = line.as_bytes();
                        let mut s = start + i;
                        while s > 0 {
                            let c = bytes[s - 1] as char;
                            if c.is_whitespace()
                                || c == '"'
                                || c == '\''
                                || c == '('
                                || c == '['
                            {
                                break;
                            }
                            s -= 1;
                        }
                        return Some((line[s..start + i + ext.len()].to_string(), n));
                    }
                }
            }
            start = end;
        }
    }
    None
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

#[cfg(not(target_arch = "wasm32"))]
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
