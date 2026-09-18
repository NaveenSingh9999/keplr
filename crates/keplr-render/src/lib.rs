use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[cfg(feature = "gpu")]
pub mod gpu;

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
    pub breadcrumbs: Vec<String>,
    pub squiggles: Vec<Squiggle>,
    pub cursors: Vec<(usize, usize)>,
    pub soft_wrap: bool,
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

fn lang_label(path: &Path) -> String {
    match keplr_lang::LangKind::from_path(path) {
        keplr_lang::LangKind::TypeScript => String::from("typescript"),
        keplr_lang::LangKind::Tsx => String::from("tsx"),
        keplr_lang::LangKind::JavaScript => String::from("javascript"),
        keplr_lang::LangKind::Cpp => String::from("cpp"),
        keplr_lang::LangKind::Go => String::from("go"),
        keplr_lang::LangKind::Rust => String::from("rust"),
        keplr_lang::LangKind::Laml => String::from("laml"),
        keplr_lang::LangKind::Python => String::from("python"),
        keplr_lang::LangKind::C => String::from("c"),
        keplr_lang::LangKind::CSharp => String::from("csharp"),
        keplr_lang::LangKind::Java => String::from("java"),
        keplr_lang::LangKind::Swift => String::from("swift"),
        keplr_lang::LangKind::Kotlin => String::from("kotlin"),
        keplr_lang::LangKind::Ruby => String::from("ruby"),
        keplr_lang::LangKind::Php => String::from("php"),
        keplr_lang::LangKind::Html => String::from("html"),
        keplr_lang::LangKind::Css => String::from("css"),
        keplr_lang::LangKind::Json => String::from("json"),
        keplr_lang::LangKind::Toml => String::from("toml"),
        keplr_lang::LangKind::Yaml => String::from("yaml"),
        keplr_lang::LangKind::Markdown => String::from("markdown"),
        keplr_lang::LangKind::Shell => String::from("shell"),
        keplr_lang::LangKind::Sql => String::from("sql"),
        keplr_lang::LangKind::Lua => String::from("lua"),
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
            if s.is_empty() {
                String::from("no-git")
            } else {
                s
            }
        }
        _ => String::from("no-git"),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
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

    let outline: Vec<String> = center_lines
        .iter()
        .take(10)
        .enumerate()
        .map(|(i, l)| format!("{} {}", i + 1, truncate(l.trim(), 48)))
        .collect();

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
            if outline.is_empty() {
                vec![String::from("(no file)")]
            } else {
                outline.clone()
            }
        }
        _ => project_lines,
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

    let right_lines: Vec<String> = {
        let lang = keplr_lang::LangKind::from_path(Path::new(&center_path));
        let syms = keplr_lang::symbols_for(lang, &center_lines);
        if syms.is_empty() {
            vec![String::from("(no symbols)")]
        } else {
            syms
        }
    };

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
            lines: right_lines,
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
    push_op(
        &mut ops,
        "titlebar.root",
        a.titlebar.root.clone(),
        b.titlebar.root.clone(),
    );
    push_op(
        &mut ops,
        "titlebar.query",
        a.titlebar.query.clone(),
        b.titlebar.query.clone(),
    );
    push_op(&mut ops, "left.title", a.left.title.clone(), b.left.title.clone());
    push_op(&mut ops, "left.active_tab", a.left.active_tab.clone(), b.left.active_tab.clone());
    push_op(
        &mut ops,
        "left.lines",
        a.left.lines.join("\n"),
        b.left.lines.join("\n"),
    );
    push_op(
        &mut ops,
        "center.path",
        a.center.path.clone(),
        b.center.path.clone(),
    );
    push_op(
        &mut ops,
        "center.lang",
        a.center.lang.clone(),
        b.center.lang.clone(),
    );
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
    push_op(
        &mut ops,
        "right.lines",
        a.right.lines.join("\n"),
        b.right.lines.join("\n"),
    );
    push_op(&mut ops, "bottom.active_tab", a.bottom.active_tab.clone(), b.bottom.active_tab.clone());
    push_op(
        &mut ops,
        "bottom.lines",
        a.bottom.lines.join("\n"),
        b.bottom.lines.join("\n"),
    );
    push_op(
        &mut ops,
        "bottom.tasks",
        format!("{:?}", a.bottom.tasks),
        format!("{:?}", b.bottom.tasks),
    );
    push_op(
        &mut ops,
        "status.branch",
        a.status.branch.clone(),
        b.status.branch.clone(),
    );
    push_op(
        &mut ops,
        "status.lsp",
        a.status.lsp.clone(),
        b.status.lsp.clone(),
    );
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

pub trait PaintBackend {
    fn paint(&self, scene: &Scene, width: usize) -> String;
}

pub struct AnsiBackend;

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
