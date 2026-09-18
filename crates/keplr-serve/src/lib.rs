use axum::{
    extract::{Query, State},
    routing::get,
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
