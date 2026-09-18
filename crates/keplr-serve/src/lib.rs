use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone)]
struct AppState {
    root: PathBuf,
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
    let text = keplr_core::buffer::Buffer::load(&full)
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

pub async fn serve(root: PathBuf, port: u16) -> anyhow::Result<()> {
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
        .with_state(AppState { root });
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
