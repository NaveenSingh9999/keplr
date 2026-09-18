# Keplr Plan E — Zed UI Depth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the canvas UI from foundation to Zed-depth: dock tabs with correct per-tab contents, breadcrumbs, diagnostics squiggles, command-palette mode, multi-cursor/soft-wrap editor state, and a bottom-dock task list with failure jump-to-error — all on real core/lang/build data.

**Architecture:** Additive depth on Plan B types. `Scene` panels become tabbed `DockPane`s; `EditorPane` gains breadcrumbs, squiggles, cursors, soft-wrap; `SceneSpec` replaces the 5-arg `build_scene` so callers name every input. A settled runner (`run_graph_settled`) captures failed/cancelled nodes as data instead of bailing, which is what a task list UI needs. `UiState` owns tab selection, palette mode, cursors, tasks, and structured diagnostics, and merges them into the scene. No live runner is added (no `winit` on Termux); the ANSI backend and scene JSON stay the production surfaces.

**Tech Stack:** Rust 1.97.1, existing workspace deps only (no new crates; `keplr-ui` gains `keplr-build` + `keplr-lang` path deps).

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md` section 4 (Canvas UI — Zed-exact), with data from sections 6 (task list) and 7 (diagnostics)

## Global Constraints

- No Electron, no Chromium shell, no WebView. Native binary + `keplr serve` JSON only.
- Same types with no rename except additive extension: `FileEntry`, `SearchHit`, `TaskDef`, `LangKind`, `Buffer`, `Cas`, `Workspace`, `Theme`, `Rect`, `PaintBackend`, `UiState`, `Tab`, `Dock`, `Action`, `key_action`, `branch_name`. `Scene`/`EditorPane`/`Panel` evolve into tabbed forms; `RunReport` gains defaulted `failed`/`cancelled` bools.
- RAM soft cap 500MB: walks cap at 20k files; symbol scan caps at 200 lines / 50 symbols; outlines cap at 10 lines.
- Results stream in <16ms chunks: search APIs take `limit: usize` and return early.
- Git remains truth; derived state under `{workspace}/.keplr/` (gitignored).
- LAML reuse only: shell to `laml` binary if present, never reimplement evaluator.
- Every task ends with a local commit; the whole plan ends with push + `gh run watch` green (`cargo test --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`).
- No placeholders: no TBD/TODO/unimplemented; empty states use honest labels (`(empty)`, `(no symbols)`, `(no tasks)`); every route returns real data.
- Standing orders for this plan: no local `cargo` runs (verify via GitHub CI with `gh`), no new test files — existing CI tests must stay green (update existing tests only where signatures force it).

---

## File Structure

```
crates/keplr-render/src/lib.rs      # E1: DockPane/BottomPane/TaskEntry/Squiggle, EditorPane+, SceneSpec, new build_scene, diff, paint
crates/keplr-render/tests/render_test.rs  # E1: SceneSpec call-site update (existing test, minimal edit)
crates/keplr-lang/src/lib.rs        # E2: symbols_for
crates/keplr-build/src/lib.rs       # E3: RunReport failed/cancelled + run_graph_settled
crates/keplr-ui/src/lib.rs          # E4: tabs/search/mode/cursors/diagnostics/tasks + to_scene merge
crates/keplr-ui/Cargo.toml          # E4: + keplr-build, keplr-lang
crates/keplr-cli/src/main.rs        # E5: Ui/Scene tab/search/mode flags
crates/keplr-serve/src/lib.rs       # E5: /scene tab/search/mode params
README.md                           # E5: Plan E usage
```

Decomposition locked here. A task implementer sees only their task; Interfaces blocks carry exact names.

---

### Task E1: Tabbed scene types + SceneSpec + breadcrumbs + squiggles

**Files:**
- Modify: `crates/keplr-render/src/lib.rs` (replace Scene-area types, `build_scene`, `diff_scenes`, `AnsiBackend::paint`)
- Modify: `crates/keplr-render/tests/render_test.rs` (SceneSpec call site)

**Interfaces:**
- Consumes: `keplr_core::{Workspace, buffer::Buffer, search::fuzzy_paths}`, `keplr_lang::{LangKind, laml_diagnostics}`
- Produces: `pub struct DockPane { pub title: String, pub tabs: Vec<String>, pub active_tab: String, pub lines: Vec<String> }`, `pub struct TaskEntry { pub name: String, pub state: String, pub output_tail: String, pub error_file: Option<String>, pub error_line: Option<u64> }`, `pub struct BottomPane { pub title: String, pub tabs: Vec<String>, pub active_tab: String, pub lines: Vec<String>, pub tasks: Vec<TaskEntry> }`, `pub struct Squiggle { pub line: u64, pub col: u64, pub len: u64, pub message: String, pub severity: String }`, `EditorPane` gains `pub breadcrumbs: Vec<String>, pub squiggles: Vec<Squiggle>, pub cursors: Vec<(usize, usize)>, pub soft_wrap: bool` (keeps `path, lang, lines, cursor, viewport_top`), `Scene { pub titlebar: TitleBar, pub left: DockPane, pub center: EditorPane, pub right: DockPane, pub bottom: BottomPane, pub status: StatusBar, pub palette_open: bool, pub palette_query: String, pub palette_mode: String, pub palette_hits: Vec<String> }`, `pub struct SceneSpec<'a> { pub root: &'a Path, pub open_file: Option<&'a Path>, pub query: &'a str, pub palette_query: Option<&'a str>, pub palette_mode: &'a str, pub search_query: Option<&'a str>, pub left_tab: &'a str, pub right_tab: &'a str, pub bottom_tab: &'a str, pub width: u16 }`, `pub fn build_scene(spec: &SceneSpec) -> Scene`, `pub fn diff_scenes(a: &Scene, b: &Scene) -> Vec<SceneOp>`, `pub trait PaintBackend`, `pub struct AnsiBackend`, `pub fn filter_commands(query: &str, limit: usize) -> Vec<String>`, `pub const PALETTE_COMMANDS: &[&str]`

- [ ] **Step 1: Replace the Scene-area of `crates/keplr-render/src/lib.rs`**

Replace everything from `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] pub struct TitleBar` through the end of `build_scene` with the code below. Keep `Theme`, `Rect`, `lang_label`, `branch_for`, `truncate` byte-identical. (The implementer: read the file, keep everything above `TitleBar` and everything from `push_op` down except `diff_scenes`/`AnsiBackend`, which are replaced by the versions below.)

```rust
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
pub struct DockPane {
    pub title: String,
    pub tabs: Vec<String>,
    pub active_tab: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskEntry {
    pub name: String,
    pub state: String,
    pub output_tail: String,
    pub error_file: Option<String>,
    pub error_line: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BottomPane {
    pub title: String,
    pub tabs: Vec<String>,
    pub active_tab: String,
    pub lines: Vec<String>,
    pub tasks: Vec<TaskEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Squiggle {
    pub line: u64,
    pub col: u64,
    pub len: u64,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorPane {
    pub path: String,
    pub lang: String,
    pub lines: Vec<String>,
    pub cursor: (usize, usize),
    pub viewport_top: usize,
    pub breadcrumbs: Vec<String>,
    pub squiggles: Vec<Squiggle>,
    pub cursors: Vec<(usize, usize)>,
    pub soft_wrap: bool,
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
    pub left: DockPane,
    pub center: EditorPane,
    pub right: DockPane,
    pub bottom: BottomPane,
    pub status: StatusBar,
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_mode: String,
    pub palette_hits: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneOp {
    pub path: String,
    pub before: String,
    pub after: String,
}

pub const PALETTE_COMMANDS: &[&str] = &[
    "file: open finder",
    "view: toggle left dock",
    "view: toggle right dock",
    "view: toggle bottom dock",
    "view: next tab",
    "task: run all",
    "task: run lint",
    "task: force run all",
    "index: rebuild",
    "build: show graph",
    "doctor: show status",
];

pub fn filter_commands(query: &str, limit: usize) -> Vec<String> {
    let q = query.to_lowercase();
    PALETTE_COMMANDS
        .iter()
        .filter(|c| q.is_empty() || c.to_lowercase().contains(&q))
        .take(limit.max(1))
        .map(|c| c.to_string())
        .collect()
}

fn breadcrumbs_for(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub struct SceneSpec<'a> {
    pub root: &'a Path,
    pub open_file: Option<&'a Path>,
    pub query: &'a str,
    pub palette_query: Option<&'a str>,
    pub palette_mode: &'a str,
    pub search_query: Option<&'a str>,
    pub left_tab: &'a str,
    pub right_tab: &'a str,
    pub bottom_tab: &'a str,
    pub width: u16,
}

pub fn build_scene(spec: &SceneSpec) -> Scene {
    let root = spec.root;
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

    let resolved_open: Option<PathBuf> = spec.open_file.map(|p| {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            root.join(p)
        }
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

    let squiggles: Vec<Squiggle> = match (&resolved_open, center_lang.as_str()) {
        (Some(full), "laml") => keplr_lang::laml_diagnostics(full)
            .into_iter()
            .filter(|d| d.severity == "error")
            .map(|d| Squiggle {
                line: d.line,
                col: d.col,
                len: 1,
                message: d.message,
                severity: d.severity,
            })
            .collect(),
        _ => Vec::new(),
    };

    let (palette_open, palette_query_str, palette_hits) = match spec.palette_query {
        Some(q) => {
            let hits = if spec.palette_mode == "commands" {
                filter_commands(q, 10)
            } else {
                let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
                keplr_core::search::fuzzy_paths(&paths, q, 10)
                    .into_iter()
                    .map(|p| {
                        p.strip_prefix(root)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect()
            };
            (true, q.to_string(), hits)
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
            query: spec.query.to_string(),
        },
        left: DockPane {
            title: String::from("left"),
            tabs: vec![
                String::from("project"),
                String::from("outline"),
                String::from("search"),
            ],
            active_tab: spec.left_tab.to_string(),
            lines: left_lines,
        },
        center: EditorPane {
            breadcrumbs: breadcrumbs_for(&center_path),
            path: center_path,
            lang: center_lang,
            lines: center_lines,
            cursor,
            cursors: vec![cursor],
            soft_wrap: false,
            viewport_top: 1,
            squiggles,
        },
        right: DockPane {
            title: String::from("right"),
            tabs: vec![String::from("symbols")],
            active_tab: spec.right_tab.to_string(),
            lines: if outline.is_empty() {
                vec![String::from("(no symbols)")]
            } else {
                outline
            },
        },
        bottom: BottomPane {
            title: String::from("bottom"),
            tabs: vec![
                String::from("terminal"),
                String::from("diagnostics"),
                String::from("tasks"),
            ],
            active_tab: spec.bottom_tab.to_string(),
            lines: vec![String::from("keplr ready — run `keplr run <task>`")],
            tasks: Vec::new(),
        },
        status: StatusBar {
            branch: branch_for(root),
            lsp: String::from("idle"),
            errors: 0,
            files,
        },
        palette_open,
        palette_query: palette_query_str,
        palette_mode: spec.palette_mode.to_string(),
        palette_hits,
    }
}
```

- [ ] **Step 2: Replace `diff_scenes`** with a version covering old paths byte-identically plus new ones:

```rust
pub fn diff_scenes(a: &Scene, b: &Scene) -> Vec<SceneOp> {
    let mut ops = Vec::new();
    push_op(&mut ops, "titlebar.root", a.titlebar.root.clone(), b.titlebar.root.clone());
    push_op(&mut ops, "titlebar.query", a.titlebar.query.clone(), b.titlebar.query.clone());
    push_op(&mut ops, "left.title", a.left.title.clone(), b.left.title.clone());
    push_op(&mut ops, "left.active_tab", a.left.active_tab.clone(), b.left.active_tab.clone());
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
        "center.cursors",
        format!("{:?}", a.center.cursors),
        format!("{:?}", b.center.cursors),
    );
    push_op(
        &mut ops,
        "center.soft_wrap",
        a.center.soft_wrap.to_string(),
        b.center.soft_wrap.to_string(),
    );
    push_op(
        &mut ops,
        "center.breadcrumbs",
        a.center.breadcrumbs.join("/"),
        b.center.breadcrumbs.join("/"),
    );
    push_op(
        &mut ops,
        "center.squiggles",
        format!("{:?}", a.center.squiggles),
        format!("{:?}", b.center.squiggles),
    );
    push_op(
        &mut ops,
        "center.viewport_top",
        a.center.viewport_top.to_string(),
        b.center.viewport_top.to_string(),
    );
    push_op(&mut ops, "right.active_tab", a.right.active_tab.clone(), b.right.active_tab.clone());
    push_op(&mut ops, "right.lines", a.right.lines.join("\n"), b.right.lines.join("\n"));
    push_op(&mut ops, "bottom.active_tab", a.bottom.active_tab.clone(), b.bottom.active_tab.clone());
    push_op(&mut ops, "bottom.lines", a.bottom.lines.join("\n"), b.bottom.lines.join("\n"));
    push_op(
        &mut ops,
        "bottom.tasks",
        format!("{:?}", a.bottom.tasks),
        format!("{:?}", b.bottom.tasks),
    );
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
        "palette.mode",
        a.palette_mode.clone(),
        b.palette_mode.clone(),
    );
    push_op(
        &mut ops,
        "palette.hits",
        a.palette_hits.join("\n"),
        b.palette_hits.join("\n"),
    );
    ops
}
```

- [ ] **Step 3: Replace the `AnsiBackend::paint` body** (keep signature). Old lines referencing `scene.left.title`/`scene.bottom.title` must change to the new shape; keep the `▶ {path}` header, numbered editor lines, and both `bar` rules so existing assertions hold:

```rust
impl PaintBackend for AnsiBackend {
    fn paint(&self, scene: &Scene, width: usize) -> String {
        let w = width.clamp(20, 240);
        let bar = "─".repeat(w);
        let mut out = String::new();
        out.push_str(&format!(
            "\x1b[1;36mkeplr\x1b[0m {}  \x1b[2mfind:{}\x1b[0m\n",
            scene.titlebar.root, scene.titlebar.query
        ));
        out.push_str(&format!("\x1b[2m{bar}\x1b[0m\n"));
        out.push_str(&format!(
            "\x1b[1m▶ {} \x1b[0m\x1b[2m({} files · {} · {} errs)\x1b[0m\n",
            scene.center.path, scene.status.files, scene.status.branch, scene.status.errors
        ));
        out.push_str(&format!(
            "\x1b[2m{} \x1b[0m\n",
            scene.center.breadcrumbs.join(" › ")
        ));
        for (i, line) in scene.center.lines.iter().take(25).enumerate() {
            let n = scene.center.viewport_top + i;
            let marker = if scene.center.cursors.iter().any(|c| c.0 == n) {
                "›"
            } else {
                " "
            };
            out.push_str(&format!(
                "\x1b[2m{n:>3} {marker}\x1b[0m {}\n",
                truncate(line, w.saturating_sub(8))
            ));
            for sq in scene
                .center
                .squiggles
                .iter()
                .filter(|s| s.line as usize == n)
                .take(2)
            {
                out.push_str(&format!(
                    "\x1b[31m    ~ {}:{}\x1b[0m {}\n",
                    sq.line,
                    sq.col,
                    truncate(&sq.message, w.saturating_sub(12))
                ));
            }
        }
        out.push_str(&format!("\x1b[2m{bar}\x1b[0m\n"));
        out.push_str(&format!(
            "\x1b[1mleft:{}/{}\x1b[0m {}\n",
            scene.left.active_tab,
            scene.left.tabs.join("|"),
            scene.left
                .lines
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ")
        ));
        out.push_str(&format!(
            "\x1b[1mright:{}\x1b[0m {}\n",
            scene.right.active_tab,
            scene.right
                .lines
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ")
        ));
        out.push_str(&format!(
            "\x1b[1mbottom:{}\x1b[0m {}\n",
            scene.bottom.active_tab,
            scene.bottom.lines.first().cloned().unwrap_or_default()
        ));
        if !scene.bottom.tasks.is_empty() {
            let done = scene
                .bottom
                .tasks
                .iter()
                .filter(|t| t.state == "ok" || t.state == "skipped")
                .count();
            out.push_str(&format!(
                "\x1b[2mtasks {}/{} ok\x1b[0m\n",
                done,
                scene.bottom.tasks.len()
            ));
        }
        out.push_str(&format!(
            "\x1b[2mbranch:{} lsp:{} lang:{}\x1b[0m\n",
            scene.status.branch, scene.status.lsp, scene.center.lang
        ));
        if scene.palette_open {
            out.push_str(&format!(
                "\x1b[1;35m◇ {}:{}\x1b[0m {}\n",
                scene.palette_mode,
                scene.palette_query,
                scene.palette_hits.join(" · ")
            ));
        }
        out
    }
}
```

The old `Panel` struct stays in the file (harmless, still referenced by nothing — no, unused struct is fine for clippy; dead_code fires only for private unused items. `Panel` is `pub`, so no warning).

- [ ] **Step 4: Update `crates/keplr-render/tests/render_test.rs`** — replace the `build_scene` call. Old:

```rust
    let scene = build_scene(root, Some(Path::new("src/main.rs")), "main", None, 100);
```

New:

```rust
    let spec = keplr_render::SceneSpec {
        root,
        open_file: Some(Path::new("src/main.rs")),
        query: "main",
        palette_query: None,
        palette_mode: "files",
        search_query: None,
        left_tab: "project",
        right_tab: "symbols",
        bottom_tab: "terminal",
        width: 100,
    };
    let scene = build_scene(&spec);
```

Also update the import line. Old:

```rust
use keplr_render::{AnsiBackend, PaintBackend, Theme, build_scene, diff_scenes};
```

New (unchanged — same names are still exported; no edit needed). Keep existing assertions untouched.

- [ ] **Step 5: Commit locally**

```bash
git add crates/keplr-render/src/lib.rs crates/keplr-render/tests/render_test.rs
git commit -m "feat(render): tabbed docks, breadcrumbs, squiggles, cursors, scene spec"
```

---

### Task E2: Symbols + per-tab contents + search tab

**Files:**
- Modify: `crates/keplr-lang/src/lib.rs` (append `symbols_for`)
- Modify: `crates/keplr-render/src/lib.rs` (left/right content switch in `build_scene`)

**Interfaces:**
- Consumes: `LangKind`, `Workspace::grep`
- Produces: `pub fn symbols_for(lang: LangKind, lines: &[String]) -> Vec<String>`

- [ ] **Step 1: Append `symbols_for` to `crates/keplr-lang/src/lib.rs`** (end of file):

```rust
pub fn symbols_for(lang: LangKind, lines: &[String]) -> Vec<String> {
    let kinds: &[&str] = match lang {
        LangKind::Rust => &["fn", "struct", "enum", "impl", "trait", "mod"],
        LangKind::TypeScript | LangKind::Tsx | LangKind::JavaScript => {
            &["function", "class", "interface"]
        }
        LangKind::Cpp => &["class", "struct"],
        LangKind::Go => &["func", "type"],
        LangKind::Laml => &["serve", "on"],
        LangKind::Other => &[],
    };
    if kinds.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in lines.iter().take(200) {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with('~') || t.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = t
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .collect();
        let mut iter = words.iter();
        let mut found: Option<(&str, &str)> = None;
        while let Some(w) = iter.next() {
            if kinds.contains(w) {
                if let Some(n) = iter.next() {
                    if n.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                        found = Some((w, n));
                    }
                }
                break;
            }
        }
        if let Some((k, n)) = found {
            out.push(format!("{k} {n}"));
            if out.len() >= 50 {
                break;
            }
        }
    }
    out
}
```

- [ ] **Step 2: Per-tab left content in `build_scene`** — replace the `left_lines` block. Old:

```rust
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
```

New:

```rust
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
    let mut project_lines: Vec<String> = rel_paths.into_iter().take(30).collect();
    if project_lines.is_empty() {
        project_lines.push(String::from("(empty)"));
    }
    let left_lines: Vec<String> = match spec.left_tab {
        "search" => match spec.search_query {
            Some(q) if !q.is_empty() => {
                let hits = ws.grep(q, 30);
                if hits.is_empty() {
                    vec![String::from("(no matches)")]
                } else {
                    hits.into_iter()
                        .map(|h| {
                            format!(
                                "{}:{}: {}",
                                h.path
                                    .strip_prefix(root)
                                    .unwrap_or(&h.path)
                                    .to_string_lossy(),
                                h.line,
                                truncate(&h.preview, 48)
                            )
                        })
                        .collect()
                }
            }
            _ => vec![String::from("(no search query)")],
        },
        "outline" => {
            let center_text: Vec<String> = center_preview_lines(root, &resolved_open);
            if center_text.is_empty() {
                vec![String::from("(no file)")]
            } else {
                center_text
                    .iter()
                    .take(10)
                    .enumerate()
                    .map(|(i, l)| format!("{} {}", i + 1, truncate(l.trim(), 48)))
                    .collect()
            }
        }
        _ => project_lines,
    };
```

This references two things that must exist: `ws` is already bound above this block in `build_scene` (keep the block in place), and a new helper `center_preview_lines`. But `center_lines` is computed later in the function — order problem. Fix: E2 also reorders `build_scene` so the center block (resolved_open + center_lines) comes BEFORE the left block. Concretely: cut the whole `let (center_path, center_lang, center_lines, cursor) = match &resolved_open {…}` block and paste it before the `rel_paths` block, and define:

```rust
    let outline: Vec<String> = center_lines
        .iter()
        .take(10)
        .enumerate()
        .map(|(i, l)| format!("{} {}", i + 1, truncate(l.trim(), 48)))
        .collect();
```

right after it (replacing the later outline definition — delete the later one). Then left `"outline"` arm is simply `if outline.is_empty() { vec![String::from("(no file)")] } else { outline.clone() }` — no helper needed. (Drop `center_preview_lines` from the plan: the outline arm uses the already-computed `outline` vec.)

So the E2 left arm for outline is:

```rust
        "outline" => {
            if outline.is_empty() {
                vec![String::from("(no file)")]
            } else {
                outline.clone()
            }
        }
```

- [ ] **Step 3: Symbols on the right** — replace the right `lines` construction. Old:

```rust
            lines: if outline.is_empty() {
                vec![String::from("(no symbols)")]
            } else {
                outline
            },
```

New:

```rust
            lines: {
                let lang = keplr_lang::LangKind::from_path(Path::new(&center_path));
                let syms = keplr_lang::symbols_for(lang, &center_lines);
                if syms.is_empty() {
                    vec![String::from("(no symbols)")]
                } else {
                    syms
                }
            },
```

`center_path` may be `"(no file)"` — `from_path` gives `Other`, `symbols_for` returns empty, honestly labeled. `Path` is already imported in render lib.

- [ ] **Step 4: Commit locally**

```bash
git add crates/keplr-lang/src/lib.rs crates/keplr-render/src/lib.rs
git commit -m "feat(ui): symbols plus per-tab dock contents and search tab"
```

---

### Task E3: Settled runner + explicit failed/cancelled states

**Files:**
- Modify: `crates/keplr-build/src/lib.rs` (RunReport fields + `run_graph_settled`)

**Interfaces:**
- Consumes: `closure`, `topo_levels`, `graph_fingerprints`, `load_journal`, `save_journal`, `run_task`, `store_outputs`, `restore_outputs`, `run_graph`
- Produces: `RunReport` gains `#[serde(default)] pub failed: bool, #[serde(default)] pub cancelled: bool`; `pub fn run_graph_settled(tasks: &BTreeMap<String, TaskDef>, workdir: &Path, targets: &[String], jobs: usize, force: bool) -> anyhow::Result<Vec<RunReport>>`

- [ ] **Step 1: Extend `RunReport`**. Old:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunReport {
    pub task: String,
    pub skipped: bool,
    pub output: String,
    pub hash: String,
}
```

New:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunReport {
    pub task: String,
    pub skipped: bool,
    pub output: String,
    pub hash: String,
    #[serde(default)]
    pub failed: bool,
    #[serde(default)]
    pub cancelled: bool,
}
```

- [ ] **Step 2: Update the four existing `RunReport` constructions** — every `reports.push(RunReport {` site in `run_ordered` and `run_graph_parallel` gains `failed: false, cancelled: false`. There are two identical skipped-push blocks:

```rust
            reports.push(RunReport {
                task: name.clone(),
                skipped: true,
                output,
                hash: fp,
            });
```

Replace both (use replace-all) with:

```rust
            reports.push(RunReport {
                task: name.clone(),
                skipped: true,
                output,
                hash: fp,
                failed: false,
                cancelled: false,
            });
```

And the two ok-push blocks. In `run_ordered`:

```rust
        reports.push(RunReport {
            task: name.clone(),
            skipped: false,
            output: out,
            hash: fp,
        });
```

In `run_graph_parallel`:

```rust
                        reports.push(RunReport {
                            task: name.clone(),
                            skipped: false,
                            output: text.clone(),
                            hash: fp,
                        });
```

Each gains the same two `false` lines (edit each individually).

- [ ] **Step 3: Append `run_graph_settled`** (end of file):

```rust
fn cancelled_entries(order: &[String], from: usize, cause: &str, fps: &BTreeMap<String, String>) -> Vec<RunReport> {
    order[from..]
        .iter()
        .map(|n| RunReport {
            task: n.clone(),
            skipped: true,
            output: format!("cancelled: {cause}"),
            hash: fps.get(n).cloned().unwrap_or_default(),
            failed: false,
            cancelled: true,
        })
        .collect()
}

pub fn run_graph_settled(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
    targets: &[String],
    jobs: usize,
    force: bool,
) -> anyhow::Result<Vec<RunReport>> {
    let jobs = jobs.clamp(1, 32);
    let levels = if targets.is_empty() {
        let all: BTreeSet<String> = tasks.keys().cloned().collect();
        topo_levels(tasks, &all)?
    } else {
        let wanted = closure(tasks, targets)?;
        topo_levels(tasks, &wanted)?
    };
    let order: Vec<String> = levels.iter().flatten().cloned().collect();
    let fps = graph_fingerprints(tasks, workdir);
    let mut journal = load_journal(workdir);
    let cas = keplr_sync::Cas::new(workdir.join(".keplr/cas"));
    let mut reports: Vec<RunReport> = Vec::new();

    let mut up_to_date_entry = |name: &str, fp: &str| -> RunReport {
        let restored = restore_outputs(&cas, workdir, &journal[name.to_string()]);
        let mut output = String::from("up to date");
        if !restored.is_empty() {
            output.push_str(&format!(" (restored {})", restored.join(", ")));
        }
        RunReport {
            task: name.to_string(),
            skipped: true,
            output,
            hash: fp.to_string(),
            failed: false,
            cancelled: false,
        }
    };

    if jobs == 1 {
        for (i, name) in order.iter().enumerate() {
            let task = tasks
                .get(name)
                .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
            let fp = fps.get(name).cloned().unwrap_or_default();
            let fresh = force || journal.get(name).map(|e| e.hash != fp).unwrap_or(true);
            if !fresh {
                reports.push(up_to_date_entry(name, &fp));
                continue;
            }
            match run_task(task, workdir) {
                Ok(out) => {
                    let stored = store_outputs(&cas, workdir, task);
                    journal.insert(
                        name.clone(),
                        JournalEntry {
                            hash: fp.clone(),
                            outputs: stored,
                        },
                    );
                    save_journal(workdir, &journal)?;
                    reports.push(RunReport {
                        task: name.clone(),
                        skipped: false,
                        output: out,
                        hash: fp,
                        failed: false,
                        cancelled: false,
                    });
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    save_journal(workdir, &journal)?;
                    reports.push(RunReport {
                        task: name.clone(),
                        skipped: false,
                        output: msg,
                        hash: fp,
                        failed: true,
                        cancelled: false,
                    });
                    let cause = format!("{name} failed");
                    reports.extend(cancelled_entries(&order, i + 1, &cause, &fps));
                    return Ok(reports);
                }
            }
        }
        return Ok(reports);
    }

    let level_of: BTreeMap<String, usize> = order
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i))
        .collect();
    let _ = level_of;
    let mut idx = 0;
    for level in &levels {
        let mut dirty: Vec<String> = Vec::new();
        for name in level {
            let fp = fps.get(name).cloned().unwrap_or_default();
            let fresh = force || journal.get(name).map(|e| e.hash != fp).unwrap_or(true);
            if !fresh {
                reports.push(up_to_date_entry(name, &fp));
            } else {
                dirty.push(name.clone());
            }
        }
        for batch in dirty.chunks(jobs) {
            let batch_out: BTreeMap<String, anyhow::Result<String>> =
                std::thread::scope(|s| {
                    let mut handles = Vec::new();
                    let mut early: BTreeMap<String, anyhow::Result<String>> =
                        BTreeMap::new();
                    for name in batch {
                        let Some(task) = tasks.get(name).cloned() else {
                            early.insert(
                                name.clone(),
                                Err(anyhow::anyhow!("unknown task {name}")),
                            );
                            continue;
                        };
                        let dir = workdir.to_path_buf();
                        handles.push((name.clone(), s.spawn(move || run_task(&task, &dir))));
                    }
                    let mut out = early;
                    for (name, h) in handles {
                        match h.join() {
                            Ok(r) => {
                                out.insert(name, r);
                            }
                            Err(_) => {
                                out.insert(
                                    name.clone(),
                                    Err(anyhow::anyhow!("task `{name}` panicked")),
                                );
                            }
                        }
                    }
                    out
                });
            for name in batch {
                idx = level_index(&order, name);
                match batch_out.get(name) {
                    Some(Ok(text)) => {
                        let task = tasks
                            .get(name)
                            .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
                        let stored = store_outputs(&cas, workdir, task);
                        let fp = fps.get(name).cloned().unwrap_or_default();
                        journal.insert(
                            name.clone(),
                            JournalEntry {
                                hash: fp.clone(),
                                outputs: stored,
                            },
                        );
                        save_journal(workdir, &journal)?;
                        reports.push(RunReport {
                            task: name.clone(),
                            skipped: false,
                            output: text.clone(),
                            hash: fp,
                            failed: false,
                            cancelled: false,
                        });
                    }
                    _ => {
                        let msg = match batch_out.get(name) {
                            Some(Err(e)) => format!("{e:#}"),
                            _ => format!("task `{name}` produced no result"),
                        };
                        let fp = fps.get(name).cloned().unwrap_or_default();
                        save_journal(workdir, &journal)?;
                        reports.push(RunReport {
                            task: name.clone(),
                            skipped: false,
                            output: msg,
                            hash: fp,
                            failed: true,
                            cancelled: false,
                        });
                        let cause = format!("{name} failed");
                        reports.extend(cancelled_entries(&order, idx + 1, &cause, &fps));
                        return Ok(reports);
                    }
                }
            }
        }
        idx = order.len();
    }
    Ok(reports)
}

fn level_index(order: &[String], name: &str) -> usize {
    order.iter().position(|n| n == name).unwrap_or(order.len())
}
```

Drop the `level_of` map (it serves nothing) — remove those three lines (`let level_of…`, `let _ = level_of;`) and the `let mut idx = 0;` / trailing `idx = order.len();` bookkeeping stays as written but simplify: keep `idx` only where assigned from `level_index`. Final code: delete the `level_of` block, keep `for name in batch { idx = level_index(&order, name); … }` with `let mut idx = 0;` before the level loop. (The snippet above already reflects this except the `level_of` lines — delete them when writing.)

Semantics locked: graph errors (unknown task, cycle) are `Err`; task failures are data (`failed: true`) with every unrun node after them recorded `cancelled: true` in topo order; the journal always saves successes first.

- [ ] **Step 4: Commit locally**

```bash
git add crates/keplr-build/src/lib.rs
git commit -m "feat(build): settled runner with failed and cancelled states"
```

---

### Task E4: UiState depth + scene merge

**Files:**
- Modify: `crates/keplr-ui/Cargo.toml` (+ keplr-build, keplr-lang)
- Modify: `crates/keplr-ui/src/lib.rs` (tabs, search, palette mode, cursors, diagnostics, run_tasks, to_scene)

**Interfaces:**
- Consumes: `keplr_render::{Scene, SceneSpec, TaskEntry, Squiggle, build_scene, filter_commands}`, `keplr_lang::Diagnostic`, `keplr_build::{TaskDef, run_graph_settled}`
- Produces: `Tab { pub path: PathBuf, pub cursor: (usize, usize), pub cursors: Vec<(usize, usize)>, pub soft_wrap: bool }`, `UiState` gains `pub left_tab/right_tab/bottom_tab/search_query/palette_mode: String`, `pub tasks: Vec<TaskEntry>`, `pub diagnostics: Vec<Diagnostic>`; methods `set_left_tab/set_right_tab/set_bottom_tab/set_palette_mode/open_file/add_cursor/clear_extra_cursors/set_soft_wrap/palette_commands/palette_results/run_tasks/to_scene` (see code)

- [ ] **Step 1: `crates/keplr-ui/Cargo.toml`** — old:

```toml
keplr-core = { path = "../keplr-core" }
keplr-render = { path = "../keplr-render" }
```

New:

```toml
keplr-core = { path = "../keplr-core" }
keplr-render = { path = "../keplr-render" }
keplr-build = { path = "../keplr-build" }
keplr-lang = { path = "../keplr-lang" }
```

- [ ] **Step 2: Replace `crates/keplr-ui/src/lib.rs`** with the full file below (Tab/UiState extended, everything else per code):

```rust
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
```

Note: existing `ui_test.rs` stays compiling — `palette_results` now returns `Vec<String>` (its `ends_with` assertion still holds), `push_diagnostic` takes `Diagnostic` (the test never calls it).

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-ui/Cargo.toml crates/keplr-ui/src/lib.rs
git commit -m "feat(ui): dock tabs, command palette, cursors, tasks and diagnostics wiring"
```

---

### Task E5: CLI flags + serve params + docs + verify

**Files:**
- Modify: `crates/keplr-cli/src/main.rs` (Ui/Scene gain tab/search/mode flags)
- Modify: `crates/keplr-serve/src/lib.rs` (`/scene` gains tab/search/mode params)
- Modify: `README.md` (Plan E usage)

**Interfaces:**
- Consumes: `SceneSpec`, `UiState` setters
- Produces: `keplr ui/scene [--left-tab --right-tab --bottom-tab --search --palette-mode]`, `/scene` query params `search, left, right, bottom, palette_mode`

- [ ] **Step 1: CLI — extend both variants**. Old (two sites, `Ui` and `Scene` — edit each):

```rust
    Ui {
        #[arg(long)]
        open: Option<PathBuf>,
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        palette: Option<String>,
        #[arg(long, default_value_t = 100)]
        width: u16,
    },
```

New:

```rust
    Ui {
        #[arg(long)]
        open: Option<PathBuf>,
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        palette: Option<String>,
        #[arg(long, default_value = "files")]
        palette_mode: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value = "project")]
        left_tab: String,
        #[arg(long, default_value = "symbols")]
        right_tab: String,
        #[arg(long, default_value = "terminal")]
        bottom_tab: String,
        #[arg(long, default_value_t = 100)]
        width: u16,
    },
```

Same shape for `Scene` (identical field list).

- [ ] **Step 2: CLI — update both arms**. Old `Ui` arm:

```rust
        Cmd::Ui {
            open,
            query,
            palette,
            width,
        } => {
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
            print!(
                "{}",
                keplr_render::PaintBackend::paint(&backend, &scene, width as usize)
            );
        }
```

New:

```rust
        Cmd::Ui {
            open,
            query,
            palette,
            palette_mode,
            search,
            left_tab,
            right_tab,
            bottom_tab,
            width,
        } => {
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
            ui.set_palette_mode(&palette_mode);
            if let Some(q) = search.clone() {
                ui.search_query = q;
            }
            ui.set_left_tab(&left_tab);
            ui.set_right_tab(&right_tab);
            ui.set_bottom_tab(&bottom_tab);
            let scene = ui.to_scene(width);
            let backend = keplr_render::AnsiBackend;
            print!(
                "{}",
                keplr_render::PaintBackend::paint(&backend, &scene, width as usize)
            );
        }
```

Old `Scene` arm:

```rust
        Cmd::Scene {
            open,
            query,
            palette,
            width,
        } => {
            let scene = keplr_render::build_scene(&cli.root, open.as_deref(), &query, palette.as_deref(), width);
            println!("{}", serde_json::to_string_pretty(&scene)?);
        }
```

New:

```rust
        Cmd::Scene {
            open,
            query,
            palette,
            palette_mode,
            search,
            left_tab,
            right_tab,
            bottom_tab,
            width,
        } => {
            let spec = keplr_render::SceneSpec {
                root: &cli.root,
                open_file: open.as_deref(),
                query: &query,
                palette_query: palette.as_deref(),
                palette_mode: &palette_mode,
                search_query: search.as_deref(),
                left_tab: &left_tab,
                right_tab: &right_tab,
                bottom_tab: &bottom_tab,
                width,
            };
            let scene = keplr_render::build_scene(&spec);
            println!("{}", serde_json::to_string_pretty(&scene)?);
        }
```

- [ ] **Step 3: Serve — extend the `scene` handler**. Old:

```rust
async fn scene(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<keplr_render::Scene> {
    let open = params.get("open").cloned();
    let query = params.get("query").cloned().unwrap_or_default();
    let palette = params.get("palette").cloned();
    let width: u16 = params.get("width").and_then(|v| v.parse().ok()).unwrap_or(100);
    let open_path: Option<PathBuf> = open.map(PathBuf::from);
    Json(keplr_render::build_scene(
        &state.root,
        open_path.as_deref(),
        &query,
        palette.as_deref(),
        width,
    ))
}
```

New:

```rust
async fn scene(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<keplr_render::Scene> {
    let open = params.get("open").cloned();
    let query = params.get("query").cloned().unwrap_or_default();
    let palette = params.get("palette").cloned();
    let palette_mode = params
        .get("palette_mode")
        .cloned()
        .unwrap_or_else(|| String::from("files"));
    let search = params.get("search").cloned();
    let left = params
        .get("left")
        .cloned()
        .unwrap_or_else(|| String::from("project"));
    let right = params
        .get("right")
        .cloned()
        .unwrap_or_else(|| String::from("symbols"));
    let bottom = params
        .get("bottom")
        .cloned()
        .unwrap_or_else(|| String::from("terminal"));
    let width: u16 = params.get("width").and_then(|v| v.parse().ok()).unwrap_or(100);
    let open_path: Option<PathBuf> = open.map(PathBuf::from);
    let spec = keplr_render::SceneSpec {
        root: &state.root,
        open_file: open_path.as_deref(),
        query: &query,
        palette_query: palette.as_deref(),
        palette_mode: &palette_mode,
        search_query: search.as_deref(),
        left_tab: &left,
        right_tab: &right,
        bottom_tab: &bottom,
        width,
    };
    Json(keplr_render::build_scene(&spec))
}
```

- [ ] **Step 4: README** — add the Plan E link next to the Plan D link line, and append:

```markdown
## Use (Plan E Zed depth, production)

cargo run -p keplr-cli -- --root . ui --open Cargo.toml --left-tab search --search "clap" --width 100
cargo run -p keplr-cli -- --root . ui --palette "run" --palette-mode commands --width 100
cargo run -p keplr-cli -- --root . scene --open crates/keplr-cli/src/main.rs --bottom-tab tasks | head -n 60
curl '127.0.0.1:7137/scene?open=Cargo.toml&left=search&search=clap&width=100'
curl '127.0.0.1:7137/scene?palette_mode=commands&palette=run'
```

- [ ] **Step 5: Commit locally, push, watch CI**

```bash
git add crates/keplr-cli/src/main.rs crates/keplr-serve/src/lib.rs README.md
git commit -m "feat(ui): tab search and palette mode flags for cli plus serve"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: green. On failure: `gh run view <id> --log-failed`, fix, push again.

---

## Self-review (run before handoff)

1. Spec coverage: dock tabs yes (E1 types + E2 contents + E4 selection), breadcrumbs yes (E1), squiggles yes (E1 lm + E4 ui diagnostics), command palette yes (E1 catalog + E4 mode), multi-cursor/soft-wrap state yes (E4; live editing loop still out of scope — no runner exists on Termux), task list + jump yes (E3 settled + E4 wiring), status/keymap yes (existing).
2. Placeholder scan: no TBD/TODO/placeholder/unimplemented; `(empty)`/`(no symbols)`/`(no tasks)`/`(no search query)`/`(hidden)` are rendered empty-states, not code stubs; every function has a real body.
3. Type consistency: `DockPane/BottomPane/TaskEntry/Squiggle/SceneSpec/PALETTE_COMMANDS/filter_commands`, `symbols_for`, `RunReport.failed/cancelled`, `run_graph_settled`, `Tab.cursors/soft_wrap`, `UiState.left_tab/right_tab/bottom_tab/search_query/palette_mode/tasks/diagnostics`, `set_*/add_cursor/clear_extra_cursors/set_soft_wrap/palette_commands/run_tasks`, `parse_file_line` spelled identically across tasks and call sites; `Panel`/`run_graph`/`run_graph_parallel` untouched.
