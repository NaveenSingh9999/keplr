use axum::{
    extract::{ConnectInfo, Query, State},
    routing::{get, post},
    Json, Router,
};
use axum::response::IntoResponse;
use serde::Serialize;
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

type DaemonMap = Arc<Mutex<HashMap<String, (Supervised, std::process::Child)>>>;

#[derive(Clone)]
struct AppState {
    root: PathBuf,
    token: String,
    attempts: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
    daemons: DaemonMap,
    sync_docs: Arc<Mutex<HashMap<String, keplr_sync::SyncDoc>>>,
    sync_tx: Arc<Mutex<HashMap<String, tokio::sync::broadcast::Sender<Vec<u8>>>>>,
    sync_peers: Arc<Mutex<HashMap<String, u32>>>,
}

#[derive(Clone)]
struct Supervised {
    cmd: String,
    started: std::time::SystemTime,
    pid: Option<u32>,
}

fn timing_safe_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

pub fn new_token() -> String {
    let bytes = std::fs::read("/dev/urandom").unwrap_or_else(|_| {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut out = Vec::new();
        let mut counter: u64 = 0;
        while out.len() < 32 {
            let mut h = DefaultHasher::new();
            std::process::id().hash(&mut h);
            std::time::SystemTime::now().hash(&mut h);
            counter.hash(&mut h);
            counter += 1;
            out.extend_from_slice(&h.finish().to_le_bytes());
        }
        out
    });
    bytes
        .iter()
        .take(32)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

pub fn resolve_token(root: &Path, flag: &str) -> String {
    if !flag.is_empty() {
        return flag.to_string();
    }
    if let Ok(t) = std::env::var("KEPLR_TOKEN") {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    let path = root.join(".keplr/token");
    if path.exists() {
        std::fs::read_to_string(&path)
            .map(|t| t.trim().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    }
}

fn locked_out(state: &AppState, addr: &SocketAddr) -> bool {
    let mut map = state.attempts.lock().unwrap_or_else(|e| e.into_inner());
    let key = addr.ip().to_string();
    if let Some((n, since)) = map.get(&key).cloned() {
        if since.elapsed().as_secs() > 60 {
            map.remove(&key);
            return false;
        }
        if n >= 10 {
            return true;
        }
    }
    false
}

fn record_fail(state: &AppState, addr: &SocketAddr) {
    let mut map = state.attempts.lock().unwrap_or_else(|e| e.into_inner());
    let key = addr.ip().to_string();
    let entry = map.entry(key).or_insert((0, Instant::now()));
    entry.0 += 1;
    if entry.0 == 1 {
        entry.1 = Instant::now();
    }
}

async fn require_token(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if state.token.is_empty() || req.uri().path() == "/" {
        return next.run(req).await;
    }
    if locked_out(&state, &addr) {
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "too many attempts"})),
        )
            .into_response();
    }
    let header_ok = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| timing_safe_eq(t.as_bytes(), state.token.as_bytes()))
        .unwrap_or(false);
    let query_ok = req.uri().query().map(|q| {
        q.split('&').any(|kv| match kv.split_once('=') {
            Some(("token", v)) => {
                timing_safe_eq(v.as_bytes(), state.token.as_bytes())
            }
            _ => false,
        })
    });
    if header_ok || query_ok.unwrap_or(false) {
        next.run(req).await
    } else {
        record_fail(&state, &addr);
        (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "unauthorized"})),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    root: String,
    branch: String,
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    let branch = keplr_core::git::branches(&state.root)
        .ok()
        .and_then(|bs| bs.into_iter().find(|b| b.current))
        .map(|b| b.name)
        .unwrap_or_else(|| String::from("no-git"));
    Json(Health {
        ok: true,
        root: state.root.display().to_string(),
        branch,
    })
}

async fn search(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<keplr_core::SearchHit>> {
    let needle = params.get("needle").cloned().unwrap_or_default();
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    let ws = keplr_core::Workspace::new(state.root);
    Json(ws.grep(&needle, limit))
}

async fn open(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let line: usize = params.get("line").and_then(|v| v.parse().ok()).unwrap_or(0);
    let full = match safe_rel(&state.root, &rel) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"error": format!("{e:#}")})),
    };
    let Ok(buf) = keplr_core::buffer::Buffer::load(full) else {
        return Json(serde_json::json!({"error": "unreadable"}));
    };
    if line == 0 {
        Json(serde_json::json!({"lines": buf.len_lines(), "text": buf.rope.to_string()}))
    } else {
        Json(serde_json::json!({"lines": buf.len_lines(), "text": buf.line(line).unwrap_or_default()}))
    }
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

async fn file_blob(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> axum::response::Response {
    use axum::body::Body;
    let rel = params.get("path").cloned().unwrap_or_default();
    let full = match safe_rel(&state.root, &rel) {
        Ok(p) => p,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("{e:#}")})),
            )
                .into_response()
        }
    };
    let bytes = match std::fs::read(&full) {
        Ok(b) => b,
        Err(_) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "unreadable"})),
            )
                .into_response()
        }
    };
    (
        [(axum::http::header::CONTENT_TYPE, mime_for(&full))],
        Body::from(bytes),
    )
        .into_response()
}

async fn files(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<keplr_core::FileEntry>> {
    let query = params.get("query").cloned().unwrap_or_default();
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    let ws = keplr_core::Workspace::new(state.root.clone());
    let entries = ws.walk_files(20_000);
    if query.is_empty() {
        return Json(entries.into_iter().take(limit).collect());
    }
    let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
    let wanted = keplr_core::search::fuzzy_paths(&paths, &query, limit);
    let out: Vec<keplr_core::FileEntry> = entries
        .into_iter()
        .filter(|e| wanted.contains(&e.path))
        .take(limit)
        .collect();
    Json(out)
}

async fn tasks(State(state): State<AppState>) -> Json<serde_json::Value> {
    let path = state.root.join("keplr.json");
    match keplr_build::load_tasks(&path) {
        Ok(map) => Json(serde_json::to_value(&map).unwrap_or(serde_json::json!({}))),
        Err(_) => Json(serde_json::json!({})),
    }
}

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

async fn tasks_graph(State(state): State<AppState>) -> Json<serde_json::Value> {
    let path = state.root.join("keplr.json");
    let map = match keplr_build::load_tasks(&path) {
        Ok(m) => m,
        Err(e) => return Json(serde_json::json!({"error": format!("{e:#}")})),
    };
    match keplr_build::topo_order(&map) {
        Ok(order) => {
            let fps = keplr_build::graph_fingerprints(&map, &state.root);
            Json(serde_json::json!({"order": order, "fingerprints": fps}))
        }
        Err(e) => Json(serde_json::json!({"error": format!("{e:#}")})),
    }
}

#[derive(serde::Deserialize)]
struct RunReq {
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    all: bool,
    #[serde(default = "one_job")]
    jobs: usize,
    #[serde(default)]
    force: bool,
}

fn one_job() -> usize {
    1
}

async fn tasks_run(
    State(state): State<AppState>,
    Json(req): Json<RunReq>,
) -> Json<serde_json::Value> {
    let path = state.root.join("keplr.json");
    let map = match keplr_build::load_tasks(&path) {
        Ok(m) => m,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    let targets: Vec<String> = if req.all || req.target.is_none() {
        Vec::new()
    } else {
        vec![req.target.clone().unwrap_or_default()]
    };
    let root = state.root.clone();
    let done = tokio::task::spawn_blocking(move || {
        if req.jobs > 1 {
            keplr_build::run_graph_parallel(&map, &root, &targets, req.jobs, req.force)
        } else {
            keplr_build::run_graph(&map, &root, &targets, req.force)
        }
    })
    .await;
    match done {
        Ok(Ok(reports)) => Json(serde_json::json!({"ok": true, "reports": reports})),
        Ok(Err(e)) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("join error: {e}")})),
    }
}

async fn index_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let ws = keplr_core::Workspace::new(state.root.clone());
    let index = keplr_core::Index::load(&ws);
    let path = ws.root.join(".keplr/index.json");
    Json(serde_json::json!({
        "files": index.len(),
        "stored": path.exists(),
        "path": path.display().to_string(),
    }))
}

async fn symbols(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let full = match safe_rel(&state.root, &rel) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"error": format!("{e:#}")})),
    };
    let lang = keplr_lang::LangKind::from_path(&full);
    let text = keplr_core::buffer::Buffer::load(full)
        .map(|b| b.rope.to_string())
        .unwrap_or_default();
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    Json(serde_json::json!({
        "file": rel,
        "lang": format!("{lang:?}"),
        "symbols": keplr_lang::symbols_detailed(lang, &lines),
    }))
}

async fn diagnostics(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let full = match safe_rel(&state.root, &rel) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"error": format!("{e:#}")})),
    };
    let lang = keplr_lang::LangKind::from_path(&full);
    let diags = if lang == keplr_lang::LangKind::Laml {
        keplr_lang::laml_diagnostics(&full)
    } else {
        match lang {
            keplr_lang::LangKind::Rust
            | keplr_lang::LangKind::Python
            | keplr_lang::LangKind::JavaScript
            | keplr_lang::LangKind::TypeScript
            | keplr_lang::LangKind::Tsx
            | keplr_lang::LangKind::Go => {
                let text = std::fs::read_to_string(&full).unwrap_or_default();
                keplr_lang::syntax_errors(lang, &full, &text)
            }
            _ => Vec::new(),
        }
    };
    Json(serde_json::json!({
        "file": rel,
        "lang": format!("{lang:?}"),
        "diagnostics": diags,
        "servers": keplr_lang::lsp_servers(lang),
    }))
}

#[derive(serde::Deserialize)]
struct DiagContentReq {
    path: String,
    #[serde(default)]
    content: String,
}

async fn diagnostics_content(
    State(_state): State<AppState>,
    Json(req): Json<DiagContentReq>,
) -> Json<serde_json::Value> {
    let full = Path::new(&req.path);
    let lang = keplr_lang::LangKind::from_path(full);
    let diags = match lang {
        keplr_lang::LangKind::Rust
        | keplr_lang::LangKind::Python
        | keplr_lang::LangKind::JavaScript
        | keplr_lang::LangKind::TypeScript
        | keplr_lang::LangKind::Tsx
        | keplr_lang::LangKind::Go => {
            keplr_lang::syntax_errors(lang, full, &req.content)
        }
        _ => Vec::new(),
    };
    Json(serde_json::json!({
        "lang": format!("{lang:?}"),
        "diagnostics": diags,
    }))
}

async fn highlight(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let line: usize = params.get("line").and_then(|v| v.parse().ok()).unwrap_or(1);
    let full = match safe_rel(&state.root, &rel) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"error": format!("{e:#}")})),
    };
    let lang = keplr_lang::LangKind::from_path(&full);
    if params.get("full").map(|v| v == "1").unwrap_or(false) {
        let text = keplr_core::buffer::Buffer::load(&full)
            .map(|b| b.rope.to_string())
            .unwrap_or_default();
        return Json(serde_json::json!({
            "file": rel,
            "lang": format!("{lang:?}"),
            "spans": keplr_lang::ts_highlight(lang, &text),
        }));
    }
    let text = keplr_core::buffer::Buffer::load(full)
        .ok()
        .and_then(|b| b.line(line))
        .unwrap_or_default();
    Json(serde_json::json!({
        "file": rel,
        "line": line,
        "lang": format!("{lang:?}"),
        "text": text,
        "spans": keplr_lang::highlight(lang, &text),
    }))
}

async fn snippets(Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
    let lang = params
        .get("lang")
        .map(|l| keplr_lang::LangKind::from_path(Path::new(&format!("x.{l}"))))
        .unwrap_or(keplr_lang::LangKind::Other);
    let prefix = params.get("prefix").cloned().unwrap_or_default();
    if prefix.is_empty() {
        Json(serde_json::json!({ "snippets": keplr_lang::snippets_for(lang) }))
    } else {
        match keplr_lang::expand_snippet(lang, &prefix) {
            Some((text, cursor)) => {
                Json(serde_json::json!({ "text": text, "cursor": cursor }))
            }
            None => Json(serde_json::json!({ "error": "no such snippet" })),
        }
    }
}

async fn git_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    match keplr_core::git::status(&state.root) {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": format!("{e:#}") })),
    }
}

async fn git_log(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(20);
    match keplr_core::git::log(&state.root, limit) {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": format!("{e:#}") })),
    }
}

async fn git_branches(State(state): State<AppState>) -> Json<serde_json::Value> {
    match keplr_core::git::branches(&state.root) {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": format!("{e:#}") })),
    }
}

async fn git_diff(State(state): State<AppState>) -> Json<serde_json::Value> {
    match keplr_core::git::diff_stat(&state.root) {
        Ok(stat) => Json(serde_json::json!({ "stat": stat })),
        Err(e) => Json(serde_json::json!({ "error": format!("{e:#}") })),
    }
}

fn safe_rel(root: &Path, rel: &str) -> anyhow::Result<PathBuf> {
    use std::path::Component;
    let p = Path::new(rel);
    if p.is_absolute() {
        anyhow::bail!("absolute paths not allowed");
    }
    let mut out = root.to_path_buf();
    for comp in p.components() {
        match comp {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => anyhow::bail!("invalid path `{rel}`"),
        }
    }
    Ok(out)
}

#[derive(serde::Deserialize)]
struct FsPathReq {
    path: String,
}

#[derive(serde::Deserialize)]
struct FsCreateReq {
    path: String,
    #[serde(default)]
    dir: bool,
}

#[derive(serde::Deserialize)]
struct FsRenameReq {
    from: String,
    to: String,
}

async fn fs_create(
    State(state): State<AppState>,
    Json(req): Json<FsCreateReq>,
) -> Json<serde_json::Value> {
    let full = match safe_rel(&state.root, &req.path) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    let done = if req.dir {
        std::fs::create_dir_all(&full).map_err(|e| anyhow::anyhow!("{e}"))
    } else {
        (|| {
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&full)?;
            Ok::<(), anyhow::Error>(())
        })()
    };
    match done {
        Ok(()) => Json(serde_json::json!({"ok": true, "path": full.display().to_string()})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn fs_rename(
    State(state): State<AppState>,
    Json(req): Json<FsRenameReq>,
) -> Json<serde_json::Value> {
    let from = match safe_rel(&state.root, &req.from) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    let to = match safe_rel(&state.root, &req.to) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    if let Some(parent) = to.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Json(serde_json::json!({"ok": false, "error": format!("{e}")}));
        }
    }
    match std::fs::rename(&from, &to) {
        Ok(()) => Json(serde_json::json!({"ok": true})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e}")})),
    }
}

async fn fs_delete(
    State(state): State<AppState>,
    Json(req): Json<FsPathReq>,
) -> Json<serde_json::Value> {
    let full = match safe_rel(&state.root, &req.path) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    let done = if full.is_dir() {
        std::fs::remove_dir_all(&full).map_err(|e| anyhow::anyhow!("{e}"))
    } else {
        std::fs::remove_file(&full).map_err(|e| anyhow::anyhow!("{e}"))
    };
    match done {
        Ok(()) => Json(serde_json::json!({"ok": true})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn fs_duplicate(
    State(state): State<AppState>,
    Json(req): Json<FsPathReq>,
) -> Json<serde_json::Value> {
    let full = match safe_rel(&state.root, &req.path) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    if !full.is_file() {
        return Json(serde_json::json!({"ok": false, "error": "only files can be duplicated"}));
    }
    let stem = full
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("copy"));
    let ext = full
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = full.parent().unwrap_or(&state.root);
    let mut n = 1u32;
    let dest = loop {
        let cand = parent.join(format!("{stem} copy{n}{ext}"));
        if !cand.exists() {
            break cand;
        }
        n += 1;
        if n > 999 {
            return Json(serde_json::json!({"ok": false, "error": "too many copies"}));
        }
    };
    match std::fs::copy(&full, &dest) {
        Ok(_) => Json(serde_json::json!({"ok": true, "path": dest.display().to_string()})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e}")})),
    }
}

#[derive(serde::Deserialize)]
struct GitPathReq {
    path: String,
}

#[derive(serde::Deserialize)]
struct GitCommitReq {
    message: String,
}

#[derive(serde::Deserialize)]
struct GitSwitchReq {
    branch: String,
    #[serde(default)]
    create: bool,
}

#[derive(serde::Deserialize)]
struct GitStashReq {
    #[serde(default)]
    message: String,
    #[serde(default)]
    pop: bool,
}

async fn git_stage(
    State(state): State<AppState>,
    Json(req): Json<GitPathReq>,
) -> Json<serde_json::Value> {
    match keplr_core::git::stage(&state.root, &req.path) {
        Ok(out) => Json(serde_json::json!({"ok": true, "output": out})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn git_unstage(
    State(state): State<AppState>,
    Json(req): Json<GitPathReq>,
) -> Json<serde_json::Value> {
    match keplr_core::git::unstage(&state.root, &req.path) {
        Ok(out) => Json(serde_json::json!({"ok": true, "output": out})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn git_discard(
    State(state): State<AppState>,
    Json(req): Json<GitPathReq>,
) -> Json<serde_json::Value> {
    match keplr_core::git::discard(&state.root, &req.path) {
        Ok(out) => Json(serde_json::json!({"ok": true, "output": out})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn git_commit(
    State(state): State<AppState>,
    Json(req): Json<GitCommitReq>,
) -> Json<serde_json::Value> {
    if req.message.trim().is_empty() {
        return Json(serde_json::json!({"ok": false, "error": "empty message"}));
    }
    match keplr_core::git::commit(&state.root, req.message.trim()) {
        Ok(out) => Json(serde_json::json!({"ok": true, "output": out})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn git_switch(
    State(state): State<AppState>,
    Json(req): Json<GitSwitchReq>,
) -> Json<serde_json::Value> {
    match keplr_core::git::switch_branch(&state.root, &req.branch, req.create) {
        Ok(out) => Json(serde_json::json!({"ok": true, "output": out})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn git_stash(
    State(state): State<AppState>,
    Json(req): Json<GitStashReq>,
) -> Json<serde_json::Value> {
    let done = if req.pop {
        keplr_core::git::stash_pop(&state.root)
    } else {
        keplr_core::git::stash_push(&state.root, &req.message)
    };
    match done {
        Ok(out) => Json(serde_json::json!({"ok": true, "output": out})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn lfs_pointer(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let full = match safe_rel(&state.root, &rel) {
        Ok(p) => p,
        Err(_) => return Json(serde_json::json!({"file": rel, "lfs": false})),
    };
    let text = std::fs::read_to_string(&full).unwrap_or_default();
    match keplr_sync::parse_lfs_pointer(&text) {
        Some(p) => Json(serde_json::json!({
            "file": rel,
            "lfs": true,
            "oid": p.oid,
            "size": p.size,
        })),
        None => Json(serde_json::json!({"file": rel, "lfs": false})),
    }
}

#[derive(serde::Deserialize)]
struct SyncMergeReq {
    #[serde(default = "default_sync_name")]
    name: String,
    #[serde(default)]
    seed: String,
    #[serde(default)]
    updates: Vec<Vec<u8>>,
}

fn default_sync_name() -> String {
    String::from("buffer")
}

async fn sync_merge(Json(req): Json<SyncMergeReq>) -> Json<serde_json::Value> {
    let doc = keplr_sync::SyncDoc::from_text(&req.name, &req.seed);
    for u in &req.updates {
        if let Err(e) = doc.apply_update(u) {
            return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")}));
        }
    }
    Json(serde_json::json!({"ok": true, "text": doc.content()}))
}

#[derive(serde::Deserialize)]
struct SnapshotSaveReq {
    name: String,
    #[serde(default)]
    update: Vec<u8>,
}

async fn sync_snapshot_save(
    State(state): State<AppState>,
    Json(req): Json<SnapshotSaveReq>,
) -> Json<serde_json::Value> {
    let dir = state.root.join(".keplr/snapshots");
    match keplr_sync::save_snapshot(&dir, &req.name, &req.update) {
        Ok(p) => Json(serde_json::json!({
            "ok": true,
            "path": p.display().to_string(),
        })),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn sync_snapshot_load(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let name = params.get("name").cloned().unwrap_or_default();
    let dir = state.root.join(".keplr/snapshots");
    match keplr_sync::load_snapshot(&dir, &name) {
        Ok(bytes) => Json(serde_json::json!({"ok": true, "name": name, "update": bytes})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

#[derive(serde::Deserialize)]
struct SaveReq {
    path: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    task: Option<String>,
}

async fn save(
    State(state): State<AppState>,
    Json(req): Json<SaveReq>,
) -> Json<serde_json::Value> {
    let ws = keplr_core::Workspace::new(state.root.clone());
    let report = match keplr_core::save_buffer(&ws, Path::new(&req.path), &req.content) {
        Ok(r) => r,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    if let Some(t) = req.task {
        let root = state.root.clone();
        let path = ws.root.join("keplr.json");
        let done = tokio::task::spawn_blocking(move || {
            keplr_build::load_tasks(&path)
                .and_then(|m| keplr_build::run_graph(&m, &root, &[t], false))
        })
        .await;
        match done {
            Ok(Ok(task_reports)) => {
                return Json(serde_json::json!({"ok": true, "report": report, "tasks": task_reports}))
            }
            Ok(Err(e)) => {
                return Json(serde_json::json!({"ok": true, "report": report, "task_error": format!("{e:#}")}))
            }
            Err(e) => {
                return Json(serde_json::json!({"ok": true, "report": report, "task_error": format!("join error: {e}")}))
            }
        }
    }
    Json(serde_json::json!({"ok": true, "report": report}))
}

async fn ui_root() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("ui.html"))
}

#[derive(Clone, Copy, Debug)]
struct TermSize {
    cols: usize,
    rows: usize,
}

impl alacritty_terminal::grid::Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Clone, Debug)]
struct TermListener;

impl alacritty_terminal::event::EventListener for TermListener {
    fn send_event(&self, _event: alacritty_terminal::event::Event) {}
}

fn term_css(c: &alacritty_terminal::vte::ansi::Color) -> Option<String> {
    use alacritty_terminal::vte::ansi::{Color, NamedColor};
    match c {
        Color::Named(n) => Some(
            match n {
                NamedColor::Black => "#000000",
                NamedColor::Red => "#f85149",
                NamedColor::Green => "#3fb950",
                NamedColor::Yellow => "#d29922",
                NamedColor::Blue => "#58a6ff",
                NamedColor::Magenta => "#bc8cff",
                NamedColor::Cyan => "#39c5cf",
                NamedColor::White => "#e6edf3",
                NamedColor::BrightBlack => "#6e7681",
                NamedColor::BrightRed => "#ff7b72",
                NamedColor::BrightGreen => "#7ee787",
                NamedColor::BrightYellow => "#ffa657",
                NamedColor::BrightBlue => "#79c0ff",
                NamedColor::BrightMagenta => "#d2a8ff",
                NamedColor::BrightCyan => "#56d4dd",
                NamedColor::BrightWhite => "#ffffff",
                NamedColor::Foreground | NamedColor::BrightForeground => "#e6edf3",
                NamedColor::Background => return None,
                NamedColor::Cursor => "#58a6ff",
                NamedColor::DimBlack => "#000000",
                NamedColor::DimRed => "#f85149",
                NamedColor::DimGreen => "#3fb950",
                NamedColor::DimYellow => "#d29922",
                NamedColor::DimBlue => "#58a6ff",
                NamedColor::DimMagenta => "#bc8cff",
                NamedColor::DimCyan => "#39c5cf",
                NamedColor::DimWhite => "#e6edf3",
                NamedColor::DimForeground => "#8b949e",
            }
            .to_string(),
        ),
        Color::Indexed(i) => Some(indexed_css(*i)),
        Color::Spec(rgb) => Some(format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b)),
    }
}

fn indexed_css(i: u8) -> String {
    const BASE: [&str; 16] = [
        "#000000", "#f85149", "#3fb950", "#d29922", "#58a6ff", "#bc8cff", "#39c5cf",
        "#e6edf3", "#6e7681", "#ff7b72", "#7ee787", "#ffa657", "#79c0ff", "#d2a8ff",
        "#56d4dd", "#ffffff",
    ];
    if i < 16 {
        return BASE[i as usize].to_string();
    }
    if i < 232 {
        let v = i - 16;
        let levels = [0, 95, 135, 175, 215, 255];
        return format!(
            "#{:02x}{:02x}{:02x}",
            levels[(v / 36) as usize],
            levels[((v % 36) / 6) as usize],
            levels[(v % 6) as usize]
        );
    }
    let g = 8 + (i - 232) * 10;
    format!("#{:02x}{:02x}{:02x}", g, g, g)
}

fn term_snapshot(
    term: &alacritty_terminal::term::Term<TermListener>,
    cols: usize,
    rows: usize,
) -> serde_json::Value {
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::cell::Flags;
    use alacritty_terminal::term::TermMode;
    let content = term.renderable_content();
    let show = content.mode.contains(TermMode::SHOW_CURSOR);
    let cur = content.cursor.point;
    let mut cells = Vec::with_capacity(rows);
    for l in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            let cell = &term.grid()[Line(l as i32)][Column(c)];
            let mut flags = 0u8;
            if cell.flags.contains(Flags::BOLD) {
                flags |= 1;
            }
            if cell.flags.contains(Flags::ITALIC) {
                flags |= 2;
            }
            if cell.flags.contains(Flags::INVERSE) {
                flags |= 4;
            }
            row.push(serde_json::json!([
                cell.c.to_string(),
                term_css(&cell.fg),
                term_css(&cell.bg),
                flags
            ]));
        }
        cells.push(row);
    }
    serde_json::json!({
        "cols": cols,
        "rows": rows,
        "cursor": [cur.line.0, cur.column.0],
        "show": show,
        "cells": cells,
    })
}

async fn push_frame(
    socket: &mut axum::extract::ws::WebSocket,
    term: &alacritty_terminal::term::Term<TermListener>,
    cols: usize,
    rows: usize,
) -> anyhow::Result<()> {
    use axum::extract::ws::Message;
    let frame = term_snapshot(term, cols, rows);
    let text = serde_json::to_string(&frame).unwrap_or_default();
    socket
        .send(Message::Text(text))
        .await
        .map_err(|e| anyhow::anyhow!("ws send failed: {e}"))?;
    Ok(())
}

async fn term_ws(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> impl axum::response::IntoResponse {
    let cols: usize = params
        .get("cols")
        .and_then(|v| v.parse().ok())
        .unwrap_or(80);
    let rows: usize = params
        .get("rows")
        .and_then(|v| v.parse().ok())
        .unwrap_or(24);
    let root = state.root.clone();
    ws.on_upgrade(move |socket| term_loop(root, cols.max(1), rows.max(1), socket))
}

async fn term_loop(
    root: PathBuf,
    cols: usize,
    rows: usize,
    mut socket: axum::extract::ws::WebSocket,
) {
    use alacritty_terminal::term::{Config, Term};
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
    let size = TermSize { cols, rows };
    let mut term = Term::new(Config::default(), &size, TermListener);
    let mut processor: Processor<StdSyncHandler> = Processor::new();
    use axum::extract::ws::Message;
    use std::io::{Read, Write};
    let pty_system = portable_pty::native_pty_system();
    let pair = match pty_system.openpty(portable_pty::PtySize {
        rows: rows as u16,
        cols: cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(_) => return,
    };
    let writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut cmd = portable_pty::CommandBuilder::new("sh");
    cmd.cwd(&root);
    let _child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(_) => return,
    };
    drop(pair.slave);
    let master = pair.master;
    let reader = match master.try_clone_reader() {
        Ok(r) => r,
        Err(_) => return,
    };
    let (fwd_tx, mut fwd_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if fwd_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let mut writer = writer;
    let mut cols = cols;
    let mut rows = rows;
    loop {
        tokio::select! {
            out = fwd_rx.recv() => {
                match out {
                    Some(bytes) => {
                        processor.advance(&mut term, &bytes);
                        if push_frame(&mut socket, &term, cols, rows).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(b))) => {
                        if writer.write_all(&b).is_err() {
                            break;
                        }
                        let _ = writer.flush();
                    }
                    Some(Ok(Message::Text(t))) => {
                        let s = t.to_string();
                        if let Some(dim) = s.strip_prefix("resize:") {
                            let mut it = dim.split('x');
                            if let (Some(c), Some(r)) = (it.next(), it.next()) {
                                if let (Ok(nc), Ok(nr)) = (c.parse::<usize>(), r.parse::<usize>()) {
                                    cols = nc.max(1);
                                    rows = nr.max(1);
                                    let _ = master.resize(portable_pty::PtySize {
                                        rows: rows as u16,
                                        cols: cols as u16,
                                        pixel_width: 0,
                                        pixel_height: 0,
                                    });
                                    term.resize(TermSize { cols, rows });
                                    if push_frame(&mut socket, &term, cols, rows).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
        }
    }
    // dropping master closes the pty; the shell exits on SIGHUP
}

pub async fn serve(root: PathBuf, port: u16) -> anyhow::Result<()> {
    serve_with_token(root, port, String::new()).await
}

pub async fn serve_with_token(root: PathBuf, port: u16, token: String) -> anyhow::Result<()> {
    serve_full(root, port, token, "127.0.0.1", true).await
}

fn spawn_daemon(
    root: &Path,
    def: &keplr_build::TaskDef,
) -> anyhow::Result<(Supervised, std::process::Child)> {
    let dir = root.join(".keplr/logs");
    std::fs::create_dir_all(&dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{}.log", def.name)))?;
    let err_log = log.try_clone()?;
    let cwd = def.cwd.as_ref().map(Path::new).unwrap_or(root);
    let child = std::process::Command::new("sh")
        .arg("-c")
        .arg(&def.cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(err_log))
        .spawn()?;
    let pid = Some(child.id());
    Ok((
        Supervised {
            cmd: def.cmd.clone(),
            started: std::time::SystemTime::now(),
            pid,
        },
        child,
    ))
}

async fn web_file(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> axum::response::Response {
    let rel = if path.is_empty() || path == "/" {
        "index.html".to_string()
    } else {
        path.trim_start_matches('/').to_string()
    };
    let full = match safe_rel(&state.root.join(".keplr/web"), &rel) {
        Ok(p) => p,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("{e:#}")})),
            )
                .into_response()
        }
    };
    let bytes = match std::fs::read(&full) {
        Ok(b) => b,
        Err(_) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "no bundle — build it (see assets/web/index.html)"})),
            )
                .into_response()
        }
    };
    let mime = match full.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html",
        "js" => "text/javascript",
        "wasm" => "application/wasm",
        "css" => "text/css",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    (
        [(axum::http::header::CONTENT_TYPE, mime)],
        axum::body::Body::from(bytes),
    )
        .into_response()
}

pub async fn serve_full(
    root: PathBuf,
    port: u16,
    token: String,
    bind: &str,
    allow_open_lan: bool,
) -> anyhow::Result<()> {
    let loopback = bind == "127.0.0.1" || bind == "::1" || bind == "localhost";
    if !loopback && token.is_empty() && !allow_open_lan {
        anyhow::bail!("refusing to serve LAN without a token (set --token, KEPLR_TOKEN, or pass --allow-open-lan)");
    }
    let state = AppState {
        root: root.clone(),
        token,
        attempts: Arc::new(Mutex::new(HashMap::new())),
        daemons: Arc::new(Mutex::new(HashMap::new())),
        sync_docs: Arc::new(Mutex::new(HashMap::new())),
        sync_tx: Arc::new(Mutex::new(HashMap::new())),
        sync_peers: Arc::new(Mutex::new(HashMap::new())),
    };
    if let Ok(tasks) = keplr_build::load_tasks(&root.join("keplr.json")) {
        for (name, def) in &tasks {
            if !def.daemon {
                continue;
            }
            match spawn_daemon(&root, def) {
                Ok((sup, child)) => {
                    state
                        .daemons
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(name.clone(), (sup, child));
                    eprintln!("keplr: daemon {name} started");
                }
                Err(e) => {
                    eprintln!("keplr: daemon {name} failed to start: {e:#}");
                }
            }
        }
    }
    let app = Router::new()
        .route("/", get(ui_root))
        .route("/health", get(health))
        .route("/search", get(search))
        .route("/open", get(open))
        .route("/file", get(file_blob))
        .route("/files", get(files))
        .route("/tasks", get(tasks))
        .route("/tasks/graph", get(tasks_graph))
        .route("/tasks/run", post(tasks_run))
        .route("/scene", get(scene))
        .route("/index/status", get(index_status))
        .route("/diagnostics", get(diagnostics))
        .route("/diagnostics/content", post(diagnostics_content))
        .route("/highlight", get(highlight))
        .route("/symbols", get(symbols))
        .route("/snippets", get(snippets))
        .route("/git/status", get(git_status))
        .route("/git/log", get(git_log))
        .route("/git/branches", get(git_branches))
        .route("/git/diff", get(git_diff))
        .route("/git/stage", post(git_stage))
        .route("/git/unstage", post(git_unstage))
        .route("/git/discard", post(git_discard))
        .route("/git/commit", post(git_commit))
        .route("/git/switch", post(git_switch))
        .route("/git/stash", post(git_stash))
        .route("/fs/create", post(fs_create))
        .route("/fs/rename", post(fs_rename))
        .route("/fs/delete", post(fs_delete))
        .route("/fs/duplicate", post(fs_duplicate))
        .route("/lfs/pointer", get(lfs_pointer))
        .route("/sync/merge", post(sync_merge))
        .route(
            "/sync/snapshot",
            post(sync_snapshot_save).get(sync_snapshot_load),
        )
        .route("/save", post(save))
        .route("/daemons", get(daemons))
        .route("/daemons/restart", post(daemon_restart))
        .route("/tasks/log", get(task_log))
        .route("/sync/channel", get(sync_channel))
        .route("/sync/status", get(sync_status))
        .route("/terms/ws", get(term_ws))
        .route("/web/*path", get(web_file))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_token,
        ))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(format!("{bind}:{port}")).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    let mut daemons = state.daemons.lock().unwrap_or_else(|e| e.into_inner());
    for (name, (_, child)) in daemons.iter_mut() {
        let _ = child.kill();
        eprintln!("keplr: stopped daemon {name}");
    }
    Ok(())
}

async fn daemons(State(state): State<AppState>) -> Json<serde_json::Value> {
    let map = state.daemons.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = map
        .iter()
        .map(|(name, (s, _))| {
            let uptime = s
                .started
                .elapsed()
                .map(|d| d.as_secs())
                .unwrap_or(0);
            serde_json::json!({ "name": name, "cmd": s.cmd, "pid": s.pid, "uptime_secs": uptime })
        })
        .collect();
    Json(serde_json::json!({ "daemons": list }))
}

#[derive(serde::Deserialize)]
struct DaemonReq {
    name: String,
}

async fn daemon_restart(
    State(state): State<AppState>,
    Json(req): Json<DaemonReq>,
) -> Json<serde_json::Value> {
    let path = state.root.join("keplr.json");
    let tasks = match keplr_build::load_tasks(&path) {
        Ok(m) => m,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    let def = match tasks.get(&req.name) {
        Some(d) => d.clone(),
        None => return Json(serde_json::json!({"ok": false, "error": "unknown task"})),
    };
    let mut map = state.daemons.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, child)) = map.get_mut(&req.name) {
        let _ = child.kill();
    }
    match spawn_daemon(&state.root, &def) {
        Ok((sup, child)) => {
            map.insert(req.name.clone(), (sup, child));
            Json(serde_json::json!({"ok": true, "name": req.name}))
        }
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn task_log(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let name = params.get("name").cloned().unwrap_or_default();
    let journal = keplr_build::load_journal(&state.root);
    match journal.get(&name) {
        Some(e) => Json(serde_json::json!({
            "name": name,
            "hash": e.hash,
            "output_tail": e.output_tail,
        })),
        None => Json(serde_json::json!({ "error": "no cached log" })),
    }
}

async fn sync_channel(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> impl axum::response::IntoResponse {
    let name = params
        .get("name")
        .cloned()
        .unwrap_or_else(|| String::from("buffer"));
    ws.on_upgrade(move |socket| channel_loop(state, name, socket))
}

async fn channel_loop(
    state: AppState,
    name: String,
    mut socket: axum::extract::ws::WebSocket,
) {
    use axum::extract::ws::Message;
    let (full, mut rx) = {
        let mut docs = state.sync_docs.lock().unwrap_or_else(|e| e.into_inner());
        let doc = docs
            .entry(name.clone())
            .or_insert_with(|| keplr_sync::SyncDoc::new(&name));
        let full = doc.encode_update();
        let mut txs = state.sync_tx.lock().unwrap_or_else(|e| e.into_inner());
        let tx = txs
            .entry(name.clone())
            .or_insert_with(|| tokio::sync::broadcast::channel(64).0)
            .clone();
        state
            .sync_peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(name.clone())
            .and_modify(|n| *n += 1)
            .or_insert(1);
        (full, tx.subscribe())
    };
    if socket.send(Message::Binary(full)).await.is_err() {
        unpeer(&state, &name);
        return;
    }
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if socket.send(Message::Ping(Vec::new())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        let docs = state.sync_docs.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(doc) = docs.get(&name) {
                            if doc.apply_update(&bytes).is_ok() {
                                let merged = doc.encode_update();
                                drop(docs);
                                let txs = state.sync_tx.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some(tx) = txs.get(&name) {
                                    // echo is idempotent; clients must not re-send on receive
                                    let _ = tx.send(merged);
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
            update = rx.recv() => {
                match update {
                    Ok(bytes) => {
                        if socket.send(Message::Binary(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }
    unpeer(&state, &name);
}

fn unpeer(state: &AppState, name: &str) {
    let mut peers = state.sync_peers.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(n) = peers.get_mut(name) {
        *n = n.saturating_sub(1);
        if *n == 0 {
            peers.remove(name);
        }
    }
}

async fn sync_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let peers = state.sync_peers.lock().unwrap_or_else(|e| e.into_inner());
    let docs = state.sync_docs.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = docs
        .keys()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "peers": peers.get(name).cloned().unwrap_or(0),
            })
        })
        .collect();
    Json(serde_json::json!({ "docs": list }))
}
