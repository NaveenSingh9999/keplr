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
    if state.token.is_empty() {
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
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(Health {
        ok: true,
        root: state.root.display().to_string(),
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

async fn diagnostics(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let full = state.root.join(&rel);
    let lang = keplr_lang::LangKind::from_path(&full);
    let diags = if lang == keplr_lang::LangKind::Laml {
        keplr_lang::laml_diagnostics(&full)
    } else {
        Vec::new()
    };
    Json(serde_json::json!({
        "file": rel,
        "lang": format!("{lang:?}"),
        "diagnostics": diags,
        "servers": keplr_lang::lsp_servers(lang),
    }))
}

async fn highlight(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let line: usize = params.get("line").and_then(|v| v.parse().ok()).unwrap_or(1);
    let full = state.root.join(&rel);
    let lang = keplr_lang::LangKind::from_path(&full);
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

async fn lfs_pointer(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let rel = params.get("path").cloned().unwrap_or_default();
    let text = std::fs::read_to_string(state.root.join(&rel)).unwrap_or_default();
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
        .route("/health", get(health))
        .route("/search", get(search))
        .route("/open", get(open))
        .route("/files", get(files))
        .route("/tasks", get(tasks))
        .route("/tasks/graph", get(tasks_graph))
        .route("/tasks/run", post(tasks_run))
        .route("/scene", get(scene))
        .route("/index/status", get(index_status))
        .route("/diagnostics", get(diagnostics))
        .route("/highlight", get(highlight))
        .route("/snippets", get(snippets))
        .route("/git/status", get(git_status))
        .route("/git/log", get(git_log))
        .route("/git/branches", get(git_branches))
        .route("/git/diff", get(git_diff))
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
        (full, tx.subscribe())
    };
    if socket.send(Message::Binary(full)).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
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
}
