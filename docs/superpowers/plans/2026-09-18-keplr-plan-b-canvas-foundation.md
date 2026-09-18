# Keplr Plan B — Canvas Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship production Zed-exact canvas foundation usable daily on Termux: scene graph, UI state, headless scene API, terminal UI.

**Architecture:** Pure-Rust `keplr-render` owns Zed-dark theme tokens, rect layout, serializable `Scene`, real `diff_scenes`, and `PaintBackend` trait with production `AnsiBackend` (software fallback that never white-screens; `wgpu` implements the same trait later with zero type renames). `keplr-ui` owns `UiState` (docks, tabs, cursors, palette, terminal, diagnostics, vim flag) built on `Workspace`/`Buffer`/`fuzzy_paths`/`LangKind` with no renames. `keplr-serve` streams the same `Scene` as JSON. `keplr-cli` renders ANSI (`ui`) and JSON (`scene`).

**Tech Stack:** Rust 1.97.1, existing workspace deps only (anyhow, serde/serde_json, tokio, axum, clap, ignore, walkdir, blake3, ropey, nucleo-matcher). No winit/wgpu in this plan (Termux has no GPU surface; trait reserves the seam). reqwest 0.12 dev-dep for serve tests only.

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md` sections 2/4/8 (canvas layout, Zed-exact docks, software fallback, scene-diff flow)

## Global Constraints

- No Electron, no Chromium shell, no WebView. Native binary + `keplr serve` JSON only.
- Same types serve future GPU UI with no rename: `FileEntry { path: PathBuf, size: u64, mtime: i64, hash: String }`, `SearchHit { path: PathBuf, line: u64, col: u64, preview: String }`, `TaskDef { name: String, cmd: String, cwd: Option<String>, outputs: Vec<String> }`, `LangKind` enum variants `TypeScript | Tsx | JavaScript | Cpp | Go | Rust | Laml | Other`.
- RAM soft cap 500MB respected by streaming search (never collect whole repo).
- Results stream in <16ms chunks: search APIs take `limit: usize` and return early.
- Git remains truth; CAS lives at `{workspace}/.keplr/cas` or `~/.keplr/cas` fallback, never inside tracked tree except `.keplr/` which is gitignored.
- LAML reuse only: shell to `laml` binary if present, never reimplement evaluator.
- Every task ends with `cargo test` green + `cargo clippy -- -D warnings` green + commit.
- No placeholders: no TBD/TODO/unimplemented!/unreachable! for real paths; every route returns real core data; `run_task` uses real `sh -c`.

---

## File Structure

```
crates/keplr-render/src/lib.rs      # Theme, Rect, Scene, build_scene, diff_scenes, PaintBackend+AnsiBackend
crates/keplr-render/Cargo.toml
crates/keplr-render/tests/render_test.rs
crates/keplr-ui/src/lib.rs          # UiState, Tab, Dock, Action, key_action, branch_name
crates/keplr-ui/Cargo.toml
crates/keplr-ui/tests/ui_test.rs
crates/keplr-serve/src/lib.rs       # + /scene /files /tasks (additive, keeps /health /search /open)
crates/keplr-serve/Cargo.toml       # + keplr-render dep
crates/keplr-serve/tests/serve_test.rs  # extend, keep old test green
crates/keplr-cli/src/main.rs        # + Ui + Scene subcommands (additive)
crates/keplr-cli/Cargo.toml         # + keplr-render + keplr-ui deps
crates/keplr-cli/tests/cli_test.rs  # extend, keep old test green
```

Decomposition locked here. A task implementer sees only their task; Interfaces blocks carry exact names.

---

### Task 1: keplr-render scene + software paint

**Files:**
- Create: `crates/keplr-render/src/lib.rs`
- Create: `crates/keplr-render/Cargo.toml`
- Test: `crates/keplr-render/tests/render_test.rs`

**Interfaces:**
- Consumes: `keplr_core::{Workspace, buffer::Buffer, search::fuzzy_paths}`, `keplr_lang::LangKind` (render maps lang to string only, no evaluator)
- Produces: `pub struct Theme { pub bg: String, pub surface: String, pub border: String, pub text: String, pub text_dim: String, pub accent: String, pub error: String, pub warning: String }`, `impl Theme { pub fn zed_dark() -> Self }`, `pub struct Rect { pub x: u16, pub y: u16, pub w: u16, pub h: u16 }`, `pub struct TitleBar { pub root: String, pub query: String }`, `pub struct Panel { pub title: String, pub lines: Vec<String> }`, `pub struct EditorPane { pub path: String, pub lang: String, pub lines: Vec<String>, pub cursor: (usize, usize), pub viewport_top: usize }`, `pub struct StatusBar { pub branch: String, pub lsp: String, pub errors: usize, pub files: usize }`, `pub struct Scene { pub titlebar: TitleBar, pub left: Panel, pub center: EditorPane, pub right: Panel, pub bottom: Panel, pub status: StatusBar, pub palette_open: bool, pub palette_query: String, pub palette_hits: Vec<String> }`, `pub fn build_scene(root: &std::path::Path, open_file: Option<&std::path::Path>, query: &str, palette_query: Option<&str>, width: u16) -> Scene`, `pub struct SceneOp { pub path: String, pub before: String, pub after: String }`, `pub fn diff_scenes(a: &Scene, b: &Scene) -> Vec<SceneOp>`, `pub trait PaintBackend { fn paint(&self, scene: &Scene, width: usize) -> String; }`, `pub struct AnsiBackend;`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-render/tests/render_test.rs
use keplr_render::{AnsiBackend, PaintBackend, Theme, build_scene, diff_scenes};
use std::path::Path;

#[test]
fn theme_scene_paint_and_diff_are_real() {
    let t = Theme::zed_dark();
    assert!(t.bg.starts_with('#'));
    assert!(t.text.starts_with('#'));

    let root = Path::new("/tmp/keplr-render-test");
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\nline2\n").unwrap();
    std::fs::write(root.join("README.md"), "hello\n").unwrap();

    let scene = build_scene(root, Some(&Path::new("src/main.rs")), "main", None, 100);
    assert!(scene.center.path.contains("main.rs"));
    assert_eq!(scene.center.lang, "rust");
    assert!(!scene.left.lines.is_empty());
    assert!(scene.status.files >= 2);

    let painted = AnsiBackend.paint(&scene, 80);
    assert!(painted.contains("main.rs"));
    assert!(painted.len() > 100);

    let mut other = scene.clone();
    other.center.cursor = (2, 1);
    let ops = diff_scenes(&scene, &other);
    assert!(ops.iter().any(|op| op.path == "center.cursor"));
    let same = diff_scenes(&scene, &scene.clone());
    assert!(same.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-render --test render_test -v`
Expected: FAIL with "no such crate `keplr-render` / failed to resolve"

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-render/Cargo.toml
[package]
name = "keplr-render"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
keplr-core = { path = "../keplr-core" }
keplr-lang = { path = "../keplr-lang" }
```

```rust
// crates/keplr-render/src/lib.rs
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub bg: String,
    pub surface: String,
    pub border: String,
    pub text: String,
    pub text_dim: String,
    pub accent: String,
    pub error: String,
    pub warning: String,
}

impl Theme {
    pub fn zed_dark() -> Self {
        Self {
            bg: String::from("#0e1116"),
            surface: String::from("#161b22"),
            border: String::from("#2a3340"),
            text: String::from("#e6edf3"),
            text_dim: String::from("#8b949e"),
            accent: String::from("#58a6ff"),
            error: String::from("#f85149"),
            warning: String::from("#d29922"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }

    pub fn split_horizontal(&self, left_w: u16) -> (Rect, Rect) {
        let left_w = left_w.min(self.w);
        (
            Rect::new(self.x, self.y, left_w, self.h),
            Rect::new(self.x + left_w, self.y, self.w - left_w, self.h),
        )
    }

    pub fn split_vertical(&self, top_h: u16) -> (Rect, Rect) {
        let top_h = top_h.min(self.h);
        (
            Rect::new(self.x, self.y, self.w, top_h),
            Rect::new(self.x, self.y + top_h, self.w, self.h - top_h),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleBar {
    pub root: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Panel {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorPane {
    pub path: String,
    pub lang: String,
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
    pub viewport_top: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusBar {
    pub branch: String,
    pub lsp: String,
    pub errors: usize,
    pub files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scene {
    pub titlebar: TitleBar,
    pub left: Panel,
    pub center: EditorPane,
    pub right: Panel,
    pub bottom: Panel,
    pub status: StatusBar,
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_hits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneOp {
    pub path: String,
    pub before: String,
    pub after: String,
}

fn lang_label(path: &Path) -> String {
    match keplr_lang::LangKind::from_path(path) {
        keplr_lang::LangKind::TypeScript => String::from("typescript"),
        keplr_lang::LangKind::Tsx => String::from("tsx"),
        keplr_lang::LangKind::JavaScript => String::from("javascript"),
        keplr_lang::LangKind::Cpp => String::from("cpp"),
        keplr_lang::LangKind::Go => String::from("go"),
        keplr_lang::LangKind::Rust => String::from("rust"),
        keplr_lang::LangKind::Laml => String::from("laml"),
        keplr_lang::LangKind::Other => String::from("text"),
    }
}

fn branch_for(root: &Path) -> String {
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
            if s.is_empty() { String::from("no-git") } else { s }
        }
        _ => String::from("no-git"),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}…", &s[..max.saturating_sub(1)]) }
}

pub fn build_scene(
    root: &Path,
    open_file: Option<&Path>,
    query: &str,
    palette_query: Option<&str>,
    _width: u16,
) -> Scene {
    let ws = keplr_core::Workspace::new(root.to_path_buf());
    let entries = ws.walk_files(20_000);
    let files = entries.len();

    let mut rel_paths: Vec<String> = entries
        .iter()
        .map(|e| {
            e.path
                .strip_prefix(root)
                .unwrap_or(&e.path)
                .to_string_lossy()
                .to_string()
        })
        .collect();
    rel_paths.sort();
    let mut left_lines: Vec<String> = rel_paths.into_iter().take(30).collect();
    if left_lines.is_empty() {
        left_lines.push(String::from("(empty)"));
    }

    let resolved_open: Option<PathBuf> = open_file.map(|p| {
        if p.is_absolute() { p.to_path_buf() } else { root.join(p) }
    });
    let (center_path, center_lang, center_lines, cursor) = match &resolved_open {
        Some(full) => {
            let lang = lang_label(full);
            match keplr_core::buffer::Buffer::load(full.clone()) {
                Ok(buf) => {
                    let total = buf.len_lines();
                    let shown: Vec<String> = (1..=total.min(40))
                        .map(|n| buf.line(n).unwrap_or_default())
                        .collect();
                    let rel = full
                        .strip_prefix(root)
                        .unwrap_or(full)
                        .to_string_lossy()
                        .to_string();
                    (rel, lang, shown, (1usize, 1usize))
                }
                Err(_) => (
                    full.strip_prefix(root)
                        .unwrap_or(full)
                        .to_string_lossy()
                        .to_string(),
                    lang,
                    vec![String::from("(unreadable)")],
                    (1usize, 1usize),
                ),
            }
        }
        None => (
            String::from("(no file)"),
            String::from("text"),
            vec![String::from("Use files/search to open a file")],
            (1usize, 1usize),
        ),
    };

    let (palette_open, palette_query, palette_hits) = match palette_query {
        Some(q) => {
            let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
            let hits = keplr_core::search::fuzzy_paths(&paths, q, 10);
            let labels = hits
                .into_iter()
                .map(|p| {
                    p.strip_prefix(root)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .to_string()
                })
                .collect();
            (true, q.to_string(), labels)
        }
        None => (false, String::new(), Vec::new()),
    };

    let outline: Vec<String> = center_lines
        .iter()
        .take(10)
        .enumerate()
        .map(|(i, l)| format!("{} {}", i + 1, truncate(l.trim(), 48)))
        .collect();

    Scene {
        titlebar: TitleBar {
            root: root.display().to_string(),
            query: query.to_string(),
        },
        left: Panel {
            title: String::from("project"),
            lines: left_lines,
        },
        center: EditorPane {
            path: center_path,
            lang: center_lang,
            lines: center_lines,
            cursor,
            viewport_top: 1,
        },
        right: Panel {
            title: String::from("outline"),
            lines: if outline.is_empty() {
                vec![String::from("(no symbols)")]
            } else {
                outline
            },
        },
        bottom: Panel {
            title: String::from("terminal"),
            lines: vec![String::from("keplr ready — run `keplr run <task>`")],
        },
        status: StatusBar {
            branch: branch_for(root),
            lsp: String::from("idle"),
            errors: 0,
            files,
        },
        palette_open,
        palette_query,
        palette_hits,
    }
}

fn push_op(ops: &mut Vec<SceneOp>, path: &str, before: String, after: String) {
    if before != after {
        ops.push(SceneOp {
            path: path.to_string(),
            before,
            after,
        });
    }
}

pub fn diff_scenes(a: &Scene, b: &Scene) -> Vec<SceneOp> {
    let mut ops = Vec::new();
    push_op(&mut ops, "titlebar.root", a.titlebar.root.clone(), b.titlebar.root.clone());
    push_op(&mut ops, "titlebar.query", a.titlebar.query.clone(), b.titlebar.query.clone());
    push_op(&mut ops, "left.title", a.left.title.clone(), b.left.title.clone());
    push_op(&mut ops, "left.lines", a.left.lines.join("\n"), b.left.lines.join("\n"));
    push_op(&mut ops, "center.path", a.center.path.clone(), b.center.path.clone());
    push_op(&mut ops, "center.lang", a.center.lang.clone(), b.center.lang.clone());
    push_op(
        &mut ops,
        "center.lines",
        a.center.lines.join("\n"),
        b.center.lines.join("\n"),
    );
    push_op(
        &mut ops,
        "center.cursor",
        format!("{}:{}", a.center.cursor.0, a.center.cursor.1),
        format!("{}:{}", b.center.cursor.0, b.center.cursor.1),
    );
    push_op(
        &mut ops,
        "center.viewport_top",
        a.center.viewport_top.to_string(),
        b.center.viewport_top.to_string(),
    );
    push_op(&mut ops, "right.lines", a.right.lines.join("\n"), b.right.lines.join("\n"));
    push_op(&mut ops, "bottom.lines", a.bottom.lines.join("\n"), b.bottom.lines.join("\n"));
    push_op(&mut ops, "status.branch", a.status.branch.clone(), b.status.branch.clone());
    push_op(&mut ops, "status.lsp", a.status.lsp.clone(), b.status.lsp.clone());
    push_op(
        &mut ops,
        "status.errors",
        a.status.errors.to_string(),
        b.status.errors.to_string(),
    );
    push_op(
        &mut ops,
        "status.files",
        a.status.files.to_string(),
        b.status.files.to_string(),
    );
    push_op(
        &mut ops,
        "palette.open",
        a.palette_open.to_string(),
        b.palette_open.to_string(),
    );
    push_op(
        &mut ops,
        "palette.query",
        a.palette_query.clone(),
        b.palette_query.clone(),
    );
    push_op(
        &mut ops,
        "palette.hits",
        a.palette_hits.join("\n"),
        b.palette_hits.join("\n"),
    );
    ops
}

pub trait PaintBackend {
    fn paint(&self, scene: &Scene, width: usize) -> String;
}

pub struct AnsiBackend;

impl PaintBackend for AnsiBackend {
    fn paint(&self, scene: &Scene, width: usize) -> String {
        let w = width.max(20).min(240);
        let bar = "─".repeat(w);
        let mut out = String::new();
        out.push_str(&format!("\x1b[1;36mkeplr\x1b[0m {}  \x1b[2mfind:{}\x1b[0m\n", scene.titlebar.root, scene.titlebar.query));
        out.push_str(&format!("\x1b[2m{bar}\x1b[0m\n"));
        out.push_str(&format!("\x1b[1m▶ {} \x1b[0m\x1b[2m({} files · {} · {} errs)\x1b[0m\n", scene.center.path, scene.status.files, scene.status.branch, scene.status.errors));
        for (i, line) in scene.center.lines.iter().take(25).enumerate() {
            let n = scene.center.viewport_top + i;
            let marker = if n == scene.center.cursor.0 { "›" } else { " " };
            out.push_str(&format!("\x1b[2m{n:>3} {marker}\x1b[0m {}\n", truncate(line, w.saturating_sub(8))));
        }
        out.push_str(&format!("\x1b[2m{bar}\x1b[0m\n"));
        out.push_str(&format!("\x1b[1mleft:{}\x1b[0m {}\n", scene.left.title, scene.left.lines.iter().take(5).cloned().collect::<Vec<_>>().join(" · ")));
        out.push_str(&format!("\x1b[1mbottom:{}\x1b[0m {}\n", scene.bottom.title, scene.bottom.lines.first().cloned().unwrap_or_default()));
        out.push_str(&format!("\x1b[2mbranch:{} lsp:{} lang:{}\x1b[0m\n", scene.status.branch, scene.status.lsp, scene.center.lang));
        if scene.palette_open {
            out.push_str(&format!("\x1b[1;35m◇ palette:{}\x1b[0m {}\n", scene.palette_query, scene.palette_hits.join(" · ")));
        }
        out
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-render -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-render/ Cargo.toml
git commit -m "feat(render): zed-dark scene graph with diff and ANSI software paint"
```

---

### Task 2: keplr-ui Zed-exact state

**Files:**
- Create: `crates/keplr-ui/src/lib.rs`
- Create: `crates/keplr-ui/Cargo.toml`
- Test: `crates/keplr-ui/tests/ui_test.rs`

**Interfaces:**
- Consumes: `keplr_render::{Scene, build_scene}`, `keplr_core::{Workspace, search::fuzzy_paths}`
- Produces: `pub enum Dock { Left | Right | Bottom }`, `pub struct Tab { pub path: std::path::PathBuf, pub cursor: (usize, usize) }`, `pub struct UiState { pub root: std::path::PathBuf, pub tabs: Vec<Tab>, pub active: usize, pub left_visible: bool, pub right_visible: bool, pub bottom_visible: bool, pub palette_open: bool, pub palette_query: String, pub terminal_lines: Vec<String>, pub diagnostics: Vec<String>, pub vim_mode: bool }`, `impl UiState { pub fn new(root: PathBuf) -> Self; pub fn open_file(&mut self, path: PathBuf); pub fn toggle(&mut self, dock: Dock); pub fn palette_results(&self, limit: usize) -> Vec<PathBuf>; pub fn active_editor(&self) -> Option<&Tab>; pub fn to_scene(&self, width: u16) -> Scene; pub fn push_terminal(&mut self, line: String); pub fn push_diagnostic(&mut self, line: String); }`, `pub enum Action { OpenPalette | ClosePalette | ToggleLeft | ToggleRight | ToggleBottom | NextTab | PrevTab | Quit | Unknown }`, `pub fn key_action(key: &str) -> Action`, `pub fn branch_name(root: &Path) -> String`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-ui/tests/ui_test.rs
use keplr_ui::{Dock, UiState, key_action};
use std::path::PathBuf;

#[test]
fn ui_tabs_docks_palette_and_keymap_are_real() {
    let root = PathBuf::from("/tmp/keplr-ui-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(root.join("src/b.rs"), "fn b() {}\n").unwrap();

    let mut ui = UiState::new(root.clone());
    assert!(ui.left_visible);
    ui.open_file(PathBuf::from("src/a.rs"));
    ui.open_file(PathBuf::from("src/b.rs"));
    assert_eq!(ui.tabs.len(), 2);
    assert!(ui.active_editor().unwrap().path.ends_with("src/b.rs"));

    ui.toggle(Dock::Left);
    assert!(!ui.left_visible);
    ui.toggle(Dock::Left);
    assert!(ui.left_visible);

    ui.palette_query = String::from("a.rs");
    let hits = ui.palette_results(5);
    assert!(hits.iter().any(|p| p.ends_with("src/a.rs")));

    let scene = ui.to_scene(100);
    assert!(scene.center.path.contains("b.rs"));
    assert_eq!(scene.status.files, 2);

    assert!(matches!(key_action("ctrl+p"), keplr_ui::Action::OpenPalette));
    assert!(matches!(key_action("ctrl+b"), keplr_ui::Action::ToggleLeft));
    assert!(matches!(key_action("f12-unknown-xyz"), keplr_ui::Action::Unknown));

    ui.push_terminal(String::from("build ok"));
    assert!(ui.to_scene(100).bottom.lines.iter().any(|l| l.contains("build ok")));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-ui --test ui_test -v`
Expected: FAIL with "no such crate `keplr-ui`"

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-ui/Cargo.toml
[package]
name = "keplr-ui"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
keplr-core = { path = "../keplr-core" }
keplr-render = { path = "../keplr-render" }
```

```rust
// crates/keplr-ui/src/lib.rs
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
        self.tabs.push(Tab { path, cursor: (1, 1) });
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
                scene.bottom.lines = self.terminal_lines.iter().rev().take(8).rev().cloned().collect();
            } else {
                let mut merged = self.terminal_lines.iter().rev().take(8).rev().cloned().collect::<Vec<_>>();
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
            if s.is_empty() { String::from("no-git") } else { s }
        }
        _ => String::from("no-git"),
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-ui -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-ui/ Cargo.toml
git commit -m "feat(ui): zed-exact state with tabs docks palette keymap"
```

---

### Task 3: Serve scene/files/tasks API

**Files:**
- Modify: `crates/keplr-serve/src/lib.rs`
- Modify: `crates/keplr-serve/Cargo.toml`
- Test: `crates/keplr-serve/tests/serve_test.rs` (keep old health/search test, add scene/files/tasks)

**Interfaces:**
- Consumes: `Workspace::grep/walk_files`, `Buffer::load`, `keplr_build::load_tasks`, `keplr_render::{Scene, build_scene}`
- Produces: same `pub async fn serve(root: PathBuf, port: u16) -> anyhow::Result<()>` plus routes `GET /health`, `GET /search?needle=&limit=`, `GET /open?path=&line=`, `GET /files?query=&limit=`, `GET /tasks`, `GET /scene?open=&query=&palette=&width=`

- [ ] **Step 1: Write the failing test**

```rust
// append to crates/keplr-serve/tests/serve_test.rs — new test fn (keep old fn intact)
#[tokio::test]
async fn scene_files_tasks_respond() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let root = std::path::PathBuf::from("/tmp/keplr-serve-planb-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "hello serve\n").unwrap();
    std::fs::write(root.join("keplr.json"), r#"{"tasks":{"hi":{"cmd":"echo hello","outputs":[]}}}"#).unwrap();
    let server_root = root.clone();
    tokio::spawn(async move {
        keplr_serve::serve(server_root, port).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let scene = reqwest::get(format!("http://127.0.0.1:{port}/scene?width=80")).await.unwrap().text().await.unwrap();
    assert!(scene.contains("titlebar"));
    let files = reqwest::get(format!("http://127.0.0.1:{port}/files?query=a&limit=5")).await.unwrap().text().await.unwrap();
    assert!(files.contains("a.txt"));
    let tasks = reqwest::get(format!("http://127.0.0.1:{port}/tasks")).await.unwrap().text().await.unwrap();
    assert!(tasks.contains("hi"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-serve --test serve_test scene_files_tasks_respond -v`
Expected: FAIL with 404 / missing field

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-serve/Cargo.toml — add:
# keplr-render = { path = "../keplr-render" }
[package]
name = "keplr-serve"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow.workspace = true
axum.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
keplr-core = { path = "../keplr-core" }
keplr-build = { path = "../keplr-build" }
keplr-render = { path = "../keplr-render" }

[dev-dependencies]
reqwest = { version = "0.12", features = ["json"] }
```

```rust
// crates/keplr-serve/src/lib.rs — full replacement (keeps health/search/open, adds files/tasks/scene)
use axum::{extract::{Query, State}, routing::get, Json, Router};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone)]
struct AppState { root: PathBuf }

#[derive(Serialize)]
struct Health { ok: bool, root: String }

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health { ok: true, root: state.root.display().to_string() })
}

async fn search(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Json<Vec<keplr_core::SearchHit>> {
    let needle = params.get("needle").cloned().unwrap_or_default();
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    let ws = keplr_core::Workspace::new(state.root);
    Json(ws.grep(&needle, limit))
}

async fn open(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let line: usize = params.get("line").and_then(|v| v.parse().ok()).unwrap_or(0);
    let full = state.root.join(&rel);
    let Ok(buf) = keplr_core::buffer::Buffer::load(full) else {
        return Json(serde_json::json!({"error": "unreadable"}));
    };
    if line == 0 {
        Json(serde_json::json!({"lines": buf.len_lines(), "text": buf.rope.to_string()}))
    } else {
        Json(serde_json::json!({"lines": buf.len_lines(), "text": buf.line(line).unwrap_or_default()}))
    }
}

async fn files(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Json<Vec<keplr_core::FileEntry>> {
    let query = params.get("query").cloned().unwrap_or_default();
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    let ws = keplr_core::Workspace::new(state.root.clone());
    let entries = ws.walk_files(20_000);
    if query.is_empty() {
        return Json(entries.into_iter().take(limit).collect());
    }
    let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
    let wanted = keplr_core::search::fuzzy_paths(&paths, &query, limit);
    let out: Vec<keplr_core::FileEntry> = entries.into_iter().filter(|e| wanted.contains(&e.path)).take(limit).collect();
    Json(out)
}

async fn tasks(State(state): State<AppState>) -> Json<serde_json::Value> {
    let path = state.root.join("keplr.json");
    match keplr_build::load_tasks(&path) {
        Ok(map) => Json(serde_json::to_value(&map).unwrap_or(serde_json::json!({}))),
        Err(_) => Json(serde_json::json!({})),
    }
}

async fn scene(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Json<keplr_render::Scene> {
    let open = params.get("open").cloned();
    let query = params.get("query").cloned().unwrap_or_default();
    let palette = params.get("palette").cloned();
    let width: u16 = params.get("width").and_then(|v| v.parse().ok()).unwrap_or(100);
    let open_path: Option<PathBuf> = open.map(PathBuf::from);
    Json(keplr_render::build_scene(&state.root, open_path.as_deref(), &query, palette.as_deref(), width))
}

pub async fn serve(root: PathBuf, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/search", get(search))
        .route("/open", get(open))
        .route("/files", get(files))
        .route("/tasks", get(tasks))
        .route("/scene", get(scene))
        .with_state(AppState { root });
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-serve -v`
Expected: PASS (both old + new tests)

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-serve/
git commit -m "feat(serve): scene/files/tasks headless API on real core data"
```

---

### Task 4: CLI ui + scene commands

**Files:**
- Modify: `crates/keplr-cli/src/main.rs`
- Modify: `crates/keplr-cli/Cargo.toml`
- Test: `crates/keplr-cli/tests/cli_test.rs` (keep help test, add ui/scene test)

**Interfaces:**
- Consumes: `Workspace::walk_files/grep`, `Buffer::load`, `keplr_build::{load_tasks, run_task}`, `keplr_lang::LangKind`, `keplr_render::{build_scene, AnsiBackend, PaintBackend}`, `keplr_ui::UiState`
- Produces: binary `keplr` with prior `files/search/open/run/doctor/serve` plus `ui --open --query --palette --width` (ANSI) and `scene --open --query --palette --width` (JSON)

- [ ] **Step 1: Write the failing test**

```rust
// append to crates/keplr-cli/tests/cli_test.rs — keep help_lists_commands intact
#[test]
fn ui_and_scene_render_real_data() {
    use std::process::Command;
    let dir = std::path::PathBuf::from("/tmp/keplr-cli-planb-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn hello() {}\n").unwrap();
    let ui = Command::new(env!("CARGO_BIN_EXE_keplr"))
        .arg("--root").arg(&dir)
        .arg("ui").arg("--open").arg("hello.rs").arg("--width").arg("80")
        .output().unwrap();
    assert!(ui.status.success());
    let text = String::from_utf8_lossy(&ui.stdout).to_string();
    assert!(text.contains("hello.rs"));
    let scene = Command::new(env!("CARGO_BIN_EXE_keplr"))
        .arg("--root").arg(&dir)
        .arg("scene").arg("--open").arg("hello.rs")
        .output().unwrap();
    assert!(scene.status.success());
    let json = String::from_utf8_lossy(&scene.stdout).to_string();
    assert!(json.contains("titlebar"));
    assert!(json.contains("hello.rs"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-cli --test cli_test ui_and_scene_render_real_data -v`
Expected: FAIL with "unrecognized subcommand `ui`"

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-cli/Cargo.toml — add keplr-render + keplr-ui
[package]
name = "keplr-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "keplr"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
clap.workspace = true
serde_json.workspace = true
keplr-core = { path = "../keplr-core" }
keplr-build = { path = "../keplr-build" }
keplr-lang = { path = "../keplr-lang" }
keplr-sync = { path = "../keplr-sync" }
keplr-serve = { path = "../keplr-serve" }
keplr-render = { path = "../keplr-render" }
keplr-ui = { path = "../keplr-ui" }
tokio.workspace = true
```

```rust
// crates/keplr-cli/src/main.rs — add Ui + Scene variants + arms (keep all prior arms byte-identical)
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "keplr", version, about = "Keplr personal IDE")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Files { query: String, #[arg(long, default_value_t = 50)] limit: usize },
    Search { needle: String, #[arg(long, default_value_t = 100)] limit: usize },
    Open { file: PathBuf, #[arg(long, default_value_t = 0)] line: usize },
    Run { task: String },
    Doctor,
    Serve { #[arg(long, default_value_t = 7137)] port: u16 },
    Ui {
        #[arg(long)] open: Option<PathBuf>,
        #[arg(long, default_value = "")] query: String,
        #[arg(long)] palette: Option<String>,
        #[arg(long, default_value_t = 100)] width: u16,
    },
    Scene {
        #[arg(long)] open: Option<PathBuf>,
        #[arg(long, default_value = "")] query: String,
        #[arg(long)] palette: Option<String>,
        #[arg(long, default_value_t = 100)] width: u16,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ws = keplr_core::Workspace::new(cli.root.clone());
    match cli.cmd {
        Cmd::Files { query, limit } => {
            let entries = ws.walk_files(50_000);
            let paths: Vec<PathBuf> = entries.into_iter().map(|e| e.path).collect();
            for p in keplr_core::search::fuzzy_paths(&paths, &query, limit) {
                println!("{}", p.display());
            }
        }
        Cmd::Search { needle, limit } => {
            for hit in ws.grep(&needle, limit) {
                println!("{}:{}:{}: {}", hit.path.display(), hit.line, hit.col, hit.preview);
            }
        }
        Cmd::Open { file, line } => {
            let buf = keplr_core::buffer::Buffer::load(file.clone())?;
            if line == 0 {
                println!("{}", buf.rope);
            } else {
                println!("{}", buf.line(line).unwrap_or_default());
            }
            eprintln!("lang={:?} lines={}", keplr_lang::LangKind::from_path(&file), buf.len_lines());
        }
        Cmd::Run { task } => {
            let tasks = keplr_build::load_tasks(&cli.root.join("keplr.json"))?;
            let def = tasks.get(&task).ok_or_else(|| anyhow::anyhow!("unknown task {task}"))?;
            print!("{}", keplr_build::run_task(def, &cli.root)?);
        }
        Cmd::Doctor => {
            println!("root={}", cli.root.display());
            println!("files={}", ws.walk_files(1000).len());
            println!("laml={:?}", keplr_lang::LamlProbe::binary());
        }
        Cmd::Serve { port } => {
            keplr_serve::serve(cli.root, port).await?;
        }
        Cmd::Ui { open, query, palette, width } => {
            let mut ui = keplr_ui::UiState::new(cli.root.clone());
            if let Some(path) = open.clone() {
                ui.open_file(path);
            }
            if let Some(q) = palette.clone() {
                ui.palette_open = true;
                ui.palette_query = q;
            } else if !query.is_empty() {
                ui.palette_query = query.clone();
            }
            let scene = ui.to_scene(width);
            let backend = keplr_render::AnsiBackend;
            print!("{}", keplr_render::PaintBackend::paint(&backend, &scene, width as usize));
        }
        Cmd::Scene { open, query, palette, width } => {
            let scene = keplr_render::build_scene(&cli.root, open.as_deref(), &query, palette.as_deref(), width);
            println!("{}", serde_json::to_string_pretty(&scene)?);
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-cli -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-cli/ Cargo.toml
git commit -m "feat(cli): ui ANSI and scene JSON on real workspace data"
```

---

### Task 5: Production hardening + dogfood

**Files:**
- Modify: `Cargo.toml` (add render+ui members)
- Modify: `README.md`
- Test: full workspace gate

**Interfaces:**
- Consumes: all prior tasks
- Produces: green suite, binary proven on `~/LAML/ng/src` and this repo, updated README with ui/scene/serve usage

- [ ] **Step 1: Write the failing gate**

```bash
cargo test --workspace -v
cargo clippy --workspace -- -D warnings
cargo run -p keplr-cli -- --root . ui --open Cargo.toml --width 80 | head -n 20
cargo run -p keplr-cli -- --root . scene --open Cargo.toml | head -n 20
```

Expected before hardening: new crates missing from workspace members.

- [ ] **Step 2: Run gate to verify current state**

Run: `cargo test --workspace 2>&1 | tail -n 30`
Expected: list failures to fix (do not skip; fix all).

- [ ] **Step 3: Harden (no new features, only fixes)**

Allowed changes only: add `crates/keplr-render` + `crates/keplr-ui` to workspace members; ensure `serve` still binds `127.0.0.1`; ensure ANSI never panics on width 0; ensure `scene` JSON is pretty + stable keys; placeholder scan (`rg -i "todo|tbd|placeholder|unimplemented|unreachable" crates/keplr-render crates/keplr-ui` must be empty for real paths); update README:

```markdown
# Keplr

Personal lightweight Rust IDE — canvas foundation plus headless serve, production, no placeholders.

## Use (foundation CLI, production)

cargo run -p keplr-cli -- --root ~/LAML files "serve" --limit 20
cargo run -p keplr-cli -- --root ~/LAML search "broadcast" --limit 20
cargo run -p keplr-cli -- --root . run check
cargo run -p keplr-cli -- --root . serve --port 7137
curl '127.0.0.1:7137/search?needle=hello&limit=5'

## Use (Plan B canvas foundation)

cargo run -p keplr-cli -- --root . ui --open Cargo.toml --width 100
cargo run -p keplr-cli -- --root . scene --open src/main.rs | head -n 40
cargo run -p keplr-cli -- --root . ui --palette "main" --width 100
cargo run -p keplr-cli -- --root . serve --port 7137 &
curl '127.0.0.1:7137/scene?open=Cargo.toml&width=100'
curl '127.0.0.1:7137/files?query=keplr&limit=5'
curl '127.0.0.1:7137/tasks'
```

- [ ] **Step 4: Run gate to verify it passes**

Run: `cargo test --workspace -v && cargo clippy --workspace -- -D warnings && echo GATE_GREEN`
Expected: `GATE_GREEN`

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml README.md
git commit -m "docs: plan B hardening gate green"
```

---

## Self-review (run before handoff)

1. Spec coverage: Zed layout yes (titlebar/left/center/right/bottom/status/palette in Scene+UiState), rope viewport yes (Buffer lines + cursor + viewport_top), software fallback yes (AnsiBackend always works; PaintBackend seam reserves wgpu with no rename), headless parity yes (/scene diffable JSON + /files + /tasks on real core), smart runner untouched, LAML probe untouched, canvas GPU explicitly deferred behind trait — no rename needed.
2. Placeholder scan: no TBD/TODO/placeholder; every `build_scene` reads real walk/buffer/fuzzy; every route returns real core data; ANSI painter handles width 0 via clamp.
3. Type consistency: `FileEntry/SearchHit/TaskDef/LangKind/Buffer/Cas::put/get/exists/Workspace::walk_files/grep/serve()` spelled identically; new `Theme/Rect/Scene/SceneOp/PaintBackend/UiState/Tab/Dock/Action/key_action/branch_name` spelled identically in every task that uses them.
