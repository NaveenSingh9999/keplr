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

    let (palette_open, palette_query_str, palette_hits) = match palette_query {
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
        palette_query: palette_query_str,
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
        "center.viewport_top",
        a.center.viewport_top.to_string(),
        b.center.viewport_top.to_string(),
    );
    push_op(
        &mut ops,
        "right.lines",
        a.right.lines.join("\n"),
        b.right.lines.join("\n"),
    );
    push_op(
        &mut ops,
        "bottom.lines",
        a.bottom.lines.join("\n"),
        b.bottom.lines.join("\n"),
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
        for (i, line) in scene.center.lines.iter().take(25).enumerate() {
            let n = scene.center.viewport_top + i;
            let marker = if n == scene.center.cursor.0 {
                "›"
            } else {
                " "
            };
            out.push_str(&format!(
                "\x1b[2m{n:>3} {marker}\x1b[0m {}\n",
                truncate(line, w.saturating_sub(8))
            ));
        }
        out.push_str(&format!("\x1b[2m{bar}\x1b[0m\n"));
        out.push_str(&format!(
            "\x1b[1mleft:{}\x1b[0m {}\n",
            scene.left.title,
            scene.left
                .lines
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ")
        ));
        out.push_str(&format!(
            "\x1b[1mbottom:{}\x1b[0m {}\n",
            scene.bottom.title,
            scene.bottom.lines.first().cloned().unwrap_or_default()
        ));
        out.push_str(&format!(
            "\x1b[2mbranch:{} lsp:{} lang:{}\x1b[0m\n",
            scene.status.branch, scene.status.lsp, scene.center.lang
        ));
        if scene.palette_open {
            out.push_str(&format!(
                "\x1b[1;35m◇ palette:{}\x1b[0m {}\n",
                scene.palette_query,
                scene.palette_hits.join(" · ")
            ));
        }
        out
    }
}
