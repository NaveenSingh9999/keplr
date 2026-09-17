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

pub async fn serve(root: PathBuf, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/search", get(search))
        .route("/open", get(open))
        .with_state(AppState { root });
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
