# Keplr Plan F — Save, Bench, Auth, TUI, GPU Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the remaining spec surface in one push: end-to-end save pipeline, bench corpus with budgets, token-gated serve, an interactive terminal editor, and a feature-gated native GPU shell — then a UI/UX polish pass.

**Architecture:** Same additive rules. `save_buffer` lives in `keplr-core` (new `keplr-sync` dep there) doing write→CAS→index→git; CLI/serve add thin edges that optionally trigger the DAG. `bench` is dependency-free (LCG synth, std timing) behind one CLI command. Auth is a bearer gate in Axum middleware with timing-safe compare, resolved flag>env>file. The TUI is a new `tui.rs` module in `keplr-cli` on `crossterm`, editing a line buffer and saving through the real pipeline. The GPU shell is a `gpu` cargo feature in `keplr-render` (`winit` window + `wgpu` layout-quad paint + resilient fallback) exposed as `keplr desktop` behind a `desktop` CLI feature; default builds and Termux are untouched. WASM browser parity is explicitly deferred (needs workspace-wide wasm-safe dep surgery on `notify`/`ignore`).

**Tech Stack:** Rust 1.97.1, existing workspace deps plus `crossterm 0.28` (CLI), and optional `winit 0.30` + `wgpu 22` + `pollster 0.3` (render `gpu` feature only).

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md` sections 2 (native shell), 4 (editor/input/resilience), 5 (hub auth), 6 (live task logs), 8 (save data flow), 10 (benches)

## Global Constraints

- No Electron, no Chromium shell, no WebView. Native binary + `keplr serve` JSON only.
- Same types with no rename except additive extension. New items: `SaveReport`, `save_buffer`, synth/bench fns, `serve_with_token`, `resolve_token`, `new_token`, TUI `edit_file`, `gpu::run_desktop`.
- RAM soft cap 500MB: bench generator caps at 50k files; trigram caps stand.
- Git remains truth; derived state under `{workspace}/.keplr/` (gitignored). Bench trees live under `.bench-corpus/` (gitignored).
- LAML reuse only: shell to `laml` binary if present, never reimplement evaluator.
- Every part ends with a local commit; the whole plan ends with push + `gh run watch` green (default job plus the new `gpu-check` job).
- No placeholders: every function real; GPU-less `desktop` falls back to the software UI (spec resilience), never a stub screen.
- Standing orders for this plan: no local `cargo` runs (verify via GitHub CI with `gh`), no new test files — existing CI tests must stay green.

---

## File Structure

```
Cargo.toml                          # + crossterm, winit, wgpu, pollster in [workspace.dependencies]
crates/keplr-core/src/lib.rs        # F1: SaveReport + save_buffer (+ keplr-sync dep); F2: synth_tree + percentile_ns
crates/keplr-core/Cargo.toml        # F1: + keplr-sync
crates/keplr-cli/src/main.rs        # F1: Save; F2: Bench; F3: Serve token flag + Token cmd; F4: Edit; F5: Desktop (cfg)
crates/keplr-cli/src/tui.rs         # F4: interactive editor (new module, part of keplr-cli)
crates/keplr-cli/Cargo.toml         # F4: + crossterm
crates/keplr-serve/src/lib.rs       # F1: POST /save; F3: token gate + serve_with_token
crates/keplr-render/src/lib.rs      # F5: #[cfg(feature = "gpu")] pub mod gpu;
crates/keplr-render/src/gpu.rs      # F5: winit shell + wgpu layout paint (new file, feature-gated)
crates/keplr-render/Cargo.toml      # F5: [features] gpu + optional deps
.github/workflows/ci.yml            # F5: gpu-check job (existing job untouched)
.gitignore                          # F2: .bench-corpus/
README.md                           # usage per part
```

---

## Part F1: End-to-end save pipeline

### Task F1-1: `save_buffer` in core

**Files:**
- Modify: `crates/keplr-core/Cargo.toml` (+ keplr-sync)
- Modify: `crates/keplr-core/src/lib.rs` (append)

**Interfaces:**
- Consumes: `Workspace::cas_dir`, `fingerprint_bytes`, `Index::{load, apply, save, len}`, `keplr_sync::Cas`
- Produces: `pub struct SaveReport { pub path: String, pub bytes: u64, pub hash: String, pub cas_stored: bool, pub index_files: usize, pub git_committed: bool, pub git_output: String }`, `pub fn save_buffer(ws: &Workspace, path: &Path, content: &str) -> anyhow::Result<SaveReport>`

- [ ] **Step 1: Add the dep.** Old `crates/keplr-core/Cargo.toml`:

```toml
[dependencies]
anyhow.workspace = true
ignore.workspace = true
walkdir.workspace = true
notify.workspace = true
blake3.workspace = true
ropey.workspace = true
nucleo-matcher.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
```

New (append one line):

```toml
[dependencies]
anyhow.workspace = true
ignore.workspace = true
walkdir.workspace = true
notify.workspace = true
blake3.workspace = true
ropey.workspace = true
nucleo-matcher.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
keplr-sync = { path = "../keplr-sync" }
```

- [ ] **Step 2: Append to `crates/keplr-core/src/lib.rs`** (end of file):

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SaveReport {
    pub path: String,
    pub bytes: u64,
    pub hash: String,
    pub cas_stored: bool,
    pub index_files: usize,
    pub git_committed: bool,
    pub git_output: String,
}

fn git_commit_file(ws: &Workspace, full: &Path) -> (bool, String) {
    if !ws.root.join(".git").exists() {
        return (false, String::from("no git repo"));
    }
    let rel = full.strip_prefix(&ws.root).unwrap_or(full);
    match std::process::Command::new("git")
        .arg("-C")
        .arg(&ws.root)
        .arg("add")
        .arg(rel)
        .output()
    {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            return (
                false,
                format!("git add failed: {}", String::from_utf8_lossy(&o.stderr)),
            )
        }
        Err(e) => return (false, format!("git add failed to spawn: {e}")),
    }
    match std::process::Command::new("git")
        .arg("-C")
        .arg(&ws.root)
        .arg("commit")
        .arg("-m")
        .arg(format!("keplr: save {}", rel.display()))
        .output()
    {
        Ok(o) => {
            let out = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            (o.status.success(), out.chars().take(300).collect())
        }
        Err(e) => (false, format!("git commit failed to spawn: {e}")),
    }
}

pub fn save_buffer(ws: &Workspace, path: &Path, content: &str) -> anyhow::Result<SaveReport> {
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        ws.root.join(path)
    };
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full, content)?;
    let bytes = content.len() as u64;
    let hash = fingerprint_bytes(content.as_bytes());
    let cas = keplr_sync::Cas::new(ws.cas_dir());
    let cas_stored = cas.put(content.as_bytes()).is_ok();
    let mut index = Index::load(ws);
    index.apply(ws, &full);
    let _ = index.save(ws);
    let index_files = index.len();
    let (git_committed, git_output) = git_commit_file(ws, &full);
    Ok(SaveReport {
        path: full.display().to_string(),
        bytes,
        hash,
        cas_stored,
        index_files,
        git_committed,
        git_output,
    })
}
```

A `git commit` that exits nonzero ("nothing to commit", missing identity) is reported honestly in the report — the save itself already succeeded.

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-core/Cargo.toml crates/keplr-core/src/lib.rs
git commit -m "feat(core): save pipeline with cas index and git"
```

### Task F1-2: CLI `save` + serve `POST /save`

**Files:**
- Modify: `crates/keplr-cli/src/main.rs` (add `Save` variant + arm)
- Modify: `crates/keplr-serve/src/lib.rs` (add `POST /save`)

**Interfaces:**
- Consumes: `save_buffer`, `run_graph`
- Produces: `keplr save <file> (--content STR | --stdin) [--task NAME]`, `POST /save {path, content, task?}`

- [ ] **Step 1: CLI variant** (after the `Open` variant):

Old:

```rust
    Open {
        file: PathBuf,
        #[arg(long, default_value_t = 0)]
        line: usize,
    },
```

New:

```rust
    Open {
        file: PathBuf,
        #[arg(long, default_value_t = 0)]
        line: usize,
    },
    Save {
        file: PathBuf,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        stdin: bool,
        #[arg(long)]
        task: Option<String>,
    },
```

- [ ] **Step 2: CLI arm** (after the `Open` arm — read it first; it prints rope/lang lines and ends before `Cmd::Run`):

```rust
        Cmd::Save {
            file,
            content,
            stdin,
            task,
        } => {
            use std::io::Read;
            let text = if let Some(c) = content {
                c
            } else if stdin {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                s
            } else {
                anyhow::bail!("save needs --content STR or --stdin");
            };
            let report = keplr_core::save_buffer(&ws, &file, &text)?;
            println!(
                "saved {} bytes={} hash={} cas={} index_files={} git={}",
                report.path,
                report.bytes,
                report.hash,
                report.cas_stored,
                report.index_files,
                report.git_committed
            );
            if !report.git_output.trim().is_empty() {
                println!("git: {}", report.git_output.lines().next().unwrap_or_default());
            }
            if let Some(t) = task {
                let tasks = keplr_build::load_tasks(&cli.root.join("keplr.json"))?;
                let reports = keplr_build::run_graph(&tasks, &cli.root, &[t], false)?;
                for r in &reports {
                    let state = if r.skipped { "skipped" } else { "ok" };
                    println!("=== {} ({state}) ===", r.task);
                    print!("{}", r.output);
                }
            }
        }
```

- [ ] **Step 3: Serve route** — append after `sync_snapshot_load`, before `pub async fn serve`:

```rust
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
```

Serve already imports `Path`/`PathBuf` and `post`. Register: `.route("/save", post(save))` next to the other post routes.

- [ ] **Step 4: Commit locally**

```bash
git add crates/keplr-cli/src/main.rs crates/keplr-serve/src/lib.rs
git commit -m "feat(save): cli save plus headless save with optional task"
```

---

## Part F2: Bench corpus + budgets

### Task F2-1: Synth generator + percentile helper + `keplr bench`

**Files:**
- Modify: `crates/keplr-core/src/lib.rs` (append `synth_tree`, `percentile_ns`)
- Modify: `crates/keplr-cli/src/main.rs` (add `Bench` variant + arm)
- Modify: `.gitignore` (ignore `.bench-corpus/`)

**Interfaces:**
- Consumes: `Workspace::walk_files/grep/grep_trigram`, `Index`, `Cas`, `run_graph`
- Produces: `pub fn synth_tree(root: &Path, files: usize, lines_per: usize) -> anyhow::Result<Vec<PathBuf>>`, `pub fn percentile_ns(samples: &[u128], pct: f64) -> u128`, `keplr bench [--files N] [--lines N] [--reuse] [--json]`

- [ ] **Step 1: Append to `crates/keplr-core/src/lib.rs`** (end of file):

```rust
fn lcg_next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

pub fn synth_tree(root: &Path, files: usize, lines_per: usize) -> anyhow::Result<Vec<PathBuf>> {
    const WORDS: &[&str] = &[
        "fn", "let", "mut", "config", "serve", "render", "index", "alpha", "beta",
        "route", "query", "cache", "state", "value", "window", "buffer", "task",
    ];
    let files = files.clamp(1, 50_000);
    let lines_per = lines_per.clamp(1, 500);
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut out = Vec::new();
    for i in 0..files {
        let dir = root.join(format!("bench/mod_{:03}", i % 64));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("file_{i:05}.rs"));
        let mut text = String::new();
        for l in 0..lines_per {
            let w1 = WORDS[(lcg_next(&mut state) as usize) % WORDS.len()];
            let w2 = WORDS[(lcg_next(&mut state) as usize) % WORDS.len()];
            let n = lcg_next(&mut state) % 1000;
            if l % 8 == 0 {
                text.push_str(&format!("fn {w1}_{w2}_{n}() {{\n"));
            } else if l % 8 == 7 {
                text.push_str("}\n");
            } else {
                text.push_str(&format!("    let {w1}_{n} = \"{w2} {n}\"; // {w2}\n"));
            }
        }
        std::fs::write(&path, &text)?;
        out.push(path);
    }
    Ok(out)
}

pub fn percentile_ns(samples: &[u128], pct: f64) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (pct / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}
```

Deterministic by construction (fixed LCG seed), so runs are comparable across machines.

- [ ] **Step 2: CLI variant** (after the `Watch` variant):

```rust
    Bench {
        #[arg(long, default_value_t = 1000)]
        files: usize,
        #[arg(long, default_value_t = 40)]
        lines: usize,
        #[arg(long)]
        reuse: bool,
        #[arg(long)]
        json: bool,
    },
```

- [ ] **Step 3: CLI arm** (after the `Watch` arm):

```rust
        Cmd::Bench {
            files,
            lines,
            reuse,
            json,
        } => {
            let corpus = cli.root.join(".bench-corpus");
            if !reuse || !corpus.exists() {
                let _ = std::fs::remove_dir_all(&corpus);
                std::fs::create_dir_all(&corpus)?;
            }
            let t0 = std::time::Instant::now();
            let made = if reuse && corpus.exists() {
                keplr_core::Workspace::new(corpus.clone()).walk_files(100_000).len()
            } else {
                keplr_core::synth_tree(&corpus, files, lines)?.len()
            };
            let gen_ms = t0.elapsed().as_millis();
            let bws = keplr_core::Workspace::new(corpus.clone());
            let t0 = std::time::Instant::now();
            let walked = bws.walk_files(100_000).len();
            let walk_ms = t0.elapsed().as_millis();
            let t0 = std::time::Instant::now();
            let bindex = {
                let b = keplr_core::Index::build(&bws);
                let _ = b.save(&bws);
                b
            };
            let index_ms = t0.elapsed().as_millis();
            let queries = ["serve", "config", "fn alpha", "route", "zzzz_no_match_zzz"];
            let mut fuzzy_ns = Vec::new();
            let mut grep_ns = Vec::new();
            for _ in 0..4 {
                for q in queries {
                    let t = std::time::Instant::now();
                    let paths: Vec<PathBuf> =
                        bindex.files().iter().map(|e| e.path.clone()).collect();
                    let _ = keplr_core::search::fuzzy_paths(&paths, q, 10);
                    fuzzy_ns.push(t.elapsed().as_nanos());
                    let t = std::time::Instant::now();
                    let _ = bws.grep_trigram(q, 10);
                    grep_ns.push(t.elapsed().as_nanos());
                }
            }
            let t0 = std::time::Instant::now();
            let cas = keplr_sync::Cas::new(bws.cas_dir());
            let blob = vec![7u8; 4096];
            let mut puts = 0;
            while t0.elapsed().as_millis() < 500 {
                let _ = cas.put(&blob)?;
                puts += 1;
            }
            let cas_ms = t0.elapsed().as_millis().max(1);
            let bench_json = cli.root.join(".bench-corpus/keplr.json");
            std::fs::write(
                &bench_json,
                r#"{"tasks":{"gen":{"cmd":"echo gen","outputs":[]},"wrap":{"cmd":"echo wrap","outputs":[],"deps":["gen"]}}}"#,
            )?;
            let btasks = keplr_build::load_tasks(&bench_json)?;
            let r1 = keplr_build::run_graph(&btasks, &corpus, &[], false)?;
            let r2 = keplr_build::run_graph(&btasks, &corpus, &[], false)?;
            let skipped = r2.iter().filter(|r| r.skipped).count();
            let report = serde_json::json!({
                "files_made": made,
                "files_walked": walked,
                "gen_ms": gen_ms,
                "walk_ms": walk_ms,
                "index_ms": index_ms,
                "fuzzy_p50_us": keplr_core::percentile_ns(&fuzzy_ns, 50.0) / 1000,
                "fuzzy_p95_us": keplr_core::percentile_ns(&fuzzy_ns, 95.0) / 1000,
                "grep_p50_us": keplr_core::percentile_ns(&grep_ns, 50.0) / 1000,
                "grep_p95_us": keplr_core::percentile_ns(&grep_ns, 95.0) / 1000,
                "cas_put_per_s": (puts as u128 * 1000) / cas_ms as u128,
                "build_rerun": r1.len(),
                "build_skipped": skipped,
            });
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("bench files_made={} walked={}", made, walked);
                println!("gen_ms={walk_ms_placeholder}");
            }
        }
```

No — write the human lines for real. Replace that last `else` with:

```rust
            } else {
                println!(
                    "bench files_made={} walked={} gen_ms={} walk_ms={} index_ms={}",
                    made, walked, gen_ms, walk_ms, index_ms
                );
                println!(
                    "fuzzy_p50_us={} fuzzy_p95_us={} grep_p50_us={} grep_p95_us={}",
                    keplr_core::percentile_ns(&fuzzy_ns, 50.0) / 1000,
                    keplr_core::percentile_ns(&fuzzy_ns, 95.0) / 1000,
                    keplr_core::percentile_ns(&grep_ns, 50.0) / 1000,
                    keplr_core::percentile_ns(&grep_ns, 95.0) / 1000
                );
                println!(
                    "cas_put_per_s={} build_rerun={} build_skipped={}",
                    (puts as u128 * 1000) / cas_ms as u128,
                    r1.len(),
                    skipped
                );
            }
```

(The implementer: use this `else` block, not the `gen_ms={walk_ms_placeholder}` line above — that line is not part of the plan.)

- [ ] **Step 4: `.gitignore`** — append `.bench-corpus/` (read the file first; it holds `/target/`, `.keplr/`, `*.log`).

- [ ] **Step 5: Commit locally**

```bash
git add crates/keplr-core/src/lib.rs crates/keplr-cli/src/main.rs .gitignore
git commit -m "feat(bench): synth corpus plus timing budgets command"
```

---

## Part F3: Token-gated serve

### Task F3-1: Token helpers + middleware + `serve_with_token`

**Files:**
- Modify: `crates/keplr-serve/src/lib.rs` (keep `serve(root, port)` byte-identical as open-mode wrapper)

**Interfaces:**
- Consumes: existing `AppState { root }` (gains `token: String`)
- Produces: `pub fn new_token() -> String`, `pub fn resolve_token(root: &Path, flag: &str) -> String`, `pub async fn serve_with_token(root: PathBuf, port: u16, token: String) -> anyhow::Result<()>`

- [ ] **Step 1: Extend `AppState`, add helpers + middleware.** Old:

```rust
#[derive(Clone)]
struct AppState {
    root: PathBuf,
}
```

New:

```rust
#[derive(Clone)]
struct AppState {
    root: PathBuf,
    token: String,
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
    root.join(".keplr/token")
        .exists()
        .then(|| {
            std::fs::read_to_string(root.join(".keplr/token"))
                .map(|t| t.trim().to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

async fn require_token(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if state.token.is_empty() {
        return next.run(req).await;
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
            Some((k, v)) if k == "token" => {
                timing_safe_eq(v.as_bytes(), state.token.as_bytes())
            }
            _ => false,
        })
    });
    if header_ok || query_ok.unwrap_or(false) {
        next.run(req).await
    } else {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "unauthorized"})),
        )
            .into_response()
    }
}
```

Needs `axum::response::IntoResponse` in scope — add `use axum::response::IntoResponse;` at the top (check current imports first; the file imports `axum::{extract::{Query, State}, routing::{get, post}, Json, Router}`).

- [ ] **Step 2: Split `serve`.** Old:

```rust
pub async fn serve(root: PathBuf, port: u16) -> anyhow::Result<()> {
    let app = Router::new()
```

New:

```rust
pub async fn serve(root: PathBuf, port: u16) -> anyhow::Result<()> {
    serve_with_token(root, port, String::new()).await
}

pub async fn serve_with_token(root: PathBuf, port: u16, token: String) -> anyhow::Result<()> {
    let state = AppState {
        root,
        token,
    };
    let app = Router::new()
```

And every `.with_state(AppState { root })` becomes `.with_state(state)` plus a `.layer(axum::middleware::from_fn_with_state(state.clone(), require_token))` before it. Read the current tail of `serve` first — it ends with the route list then `.with_state(AppState { root });`. Replace that ending with:

```rust
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_token,
        ))
        .with_state(state);
```

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-serve/src/lib.rs
git commit -m "feat(serve): bearer token gate with open-mode default"
```

### Task F3-2: CLI token flag + `keplr token`

**Files:**
- Modify: `crates/keplr-cli/src/main.rs` (Serve gains `--token`; add `Token` variant + arm)

**Interfaces:**
- Consumes: `serve_with_token`, `resolve_token`, `new_token`
- Produces: `keplr serve --token STR` (else `KEPLR_TOKEN` env, else `.keplr/token`), `keplr token [--save]`

- [ ] **Step 1: Extend `Serve`.** Old:

```rust
    Serve {
        #[arg(long, default_value_t = 7137)]
        port: u16,
    },
```

New:

```rust
    Serve {
        #[arg(long, default_value_t = 7137)]
        port: u16,
        #[arg(long, default_value = "")]
        token: String,
    },
    Token {
        #[arg(long)]
        save: bool,
    },
```

- [ ] **Step 2: Update the `Serve` arm.** Old:

```rust
        Cmd::Serve { port } => {
            keplr_serve::serve(cli.root, port).await?;
        }
```

New:

```rust
        Cmd::Serve { port, token } => {
            let resolved = keplr_serve::resolve_token(&cli.root, &token);
            if resolved.is_empty() {
                eprintln!("keplr: no token configured — serving open on 127.0.0.1");
            } else {
                eprintln!("keplr: token gate enabled");
            }
            keplr_serve::serve_with_token(cli.root, port, resolved).await?;
        }
        Cmd::Token { save } => {
            let token = keplr_serve::new_token();
            if save {
                let dir = cli.root.join(".keplr");
                std::fs::create_dir_all(&dir)?;
                let path = dir.join("token");
                std::fs::write(&path, format!("{token}\n"))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &path,
                        std::fs::Permissions::from_mode(0o600),
                    );
                }
                println!("saved to {}", path.display());
            } else {
                println!("{token}");
            }
        }
```

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-cli/src/main.rs
git commit -m "feat(cli): serve token flag plus token generator"
```

---

## Part F4: Interactive terminal editor

### Task F4-1: `tui.rs` editor module + `Edit` command

**Files:**
- Create: `crates/keplr-cli/src/tui.rs`
- Modify: `crates/keplr-cli/src/main.rs` (`mod tui;` + `Edit` variant + arm)
- Modify: `crates/keplr-cli/Cargo.toml` (+ crossterm)

**Interfaces:**
- Consumes: `save_buffer`, `Workspace::walk_files`, `search::fuzzy_paths`, `LangKind`, `branch_name`
- Produces: `pub fn edit_file(root: PathBuf, file: PathBuf) -> anyhow::Result<()>`, `keplr edit <file>`

- [ ] **Step 1: Workspace dep.** Add to `Cargo.toml` `[workspace.dependencies]`: `crossterm = "0.28"`. Add to `crates/keplr-cli/Cargo.toml` `[dependencies]`: `crossterm.workspace = true`.

- [ ] **Step 2: Create `crates/keplr-cli/src/tui.rs`** with the full module below (line buffer on `Vec<String>`, char-safe columns, palette overlay, save through the real pipeline):

```rust
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::time::Duration;

struct ScreenGuard;

impl ScreenGuard {
    fn enter() -> anyhow::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, Hide)?;
        Ok(Self)
    }
}

impl Drop for ScreenGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
    }
}

struct Doc {
    lines: Vec<String>,
    ends_nl: bool,
}

impl Doc {
    fn load(full: &Path) -> Self {
        let text = std::fs::read_to_string(full).unwrap_or_default();
        let ends_nl = text.ends_with('\n');
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self { lines, ends_nl }
    }

    fn content(&self) -> String {
        let mut s = self.lines.join("\n");
        if self.ends_nl {
            s.push('\n');
        }
        s
    }

    fn byte_col(&self, line: usize, col: usize) -> usize {
        self.lines
            .get(line)
            .and_then(|l| l.char_indices().nth(col).map(|(i, _)| i))
            .unwrap_or_else(|| self.lines.get(line).map(|l| l.len()).unwrap_or(0))
    }

    fn line_chars(&self, line: usize) -> usize {
        self.lines.get(line).map(|l| l.chars().count()).unwrap_or(0)
    }
}

fn draw(
    doc: &Doc,
    file_label: &str,
    branch: &str,
    lang: &str,
    cursor: (usize, usize),
    top: usize,
    dirty: bool,
    status: &str,
    palette_open: bool,
    palette_query: &str,
    palette_hits: &[String],
    palette_sel: usize,
) -> anyhow::Result<()> {
    let (cols, rows) = terminal::size()?;
    let mut out = stdout();
    execute!(out, MoveTo(0, 0), Clear(ClearType::All))?;
    let dot = if dirty { "●" } else { " " };
    execute!(
        out,
        Print(format!(
            "\x1b[1;36mkeplr\x1b[0m {file_label} {dot} \x1b[2m{branch} {lang}\x1b[0m\r\n"
        ))
    )?;
    let height = rows.saturating_sub(3) as usize;
    for i in 0..height {
        let idx = top + i;
        if let Some(line) = doc.lines.get(idx) {
            let shown = truncate_cells(line, cols.saturating_sub(7) as usize);
            execute!(
                out,
                Print(format!("\x1b[2m{:>4} \x1b[0m{shown}\r\n", idx + 1))
            )?;
        } else {
            execute!(out, Print("~\r\n"))?;
        }
    }
    execute!(
        out,
        Print(format!(
            "\x1b[7m {:<w$} \x1b[0m\r\n",
            status,
            w = cols.saturating_sub(2) as usize
        ))
    )?;
    if palette_open {
        execute!(
            out,
            MoveTo(0, 1),
            Clear(ClearType::CurrentLine),
            Print(format!("› {palette_query}\r\n"))
        )?;
        for (i, h) in palette_hits.iter().take(8).enumerate() {
            let mark = if i == palette_sel { "▸" } else { " " };
            let shown = truncate_cells(h, cols.saturating_sub(4) as usize);
            if i == palette_sel {
                execute!(out, Print(format!("\x1b[7m{mark} {shown}\x1b[0m\r\n")))?;
            } else {
                execute!(out, Print(format!("{mark} {shown}\r\n")))?;
            }
        }
    }
    let crow = 1 + cursor.0.saturating_sub(top);
    let ccol = 5 + cursor.1;
    execute!(out, MoveTo(ccol as u16, crow as u16))?;
    out.flush()?;
    Ok(())
}

fn truncate_cells(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

pub fn edit_file(root: PathBuf, file: PathBuf) -> anyhow::Result<()> {
    let full = if file.is_absolute() {
        file
    } else {
        root.join(&file)
    };
    let ws = keplr_core::Workspace::new(root.clone());
    let file_label = full
        .strip_prefix(&root)
        .unwrap_or(&full)
        .display()
        .to_string();
    let lang = format!("{:?}", keplr_lang::LangKind::from_path(&full));
    let branch = keplr_ui::branch_name(&root);
    let mut doc = Doc::load(&full);
    let mut cursor = (0usize, 0usize);
    let mut top = 0usize;
    let mut dirty = false;
    let mut status = String::from("ctrl+s save · ctrl+p finder · ctrl+q quit");
    let mut quit_armed = false;
    let mut palette_open = false;
    let mut palette_query = String::new();
    let mut palette_hits: Vec<String> = Vec::new();
    let mut palette_sel = 0usize;
    let mut index: Vec<PathBuf> = Vec::new();
    let _guard = ScreenGuard::enter()?;

    let refresh_palette = |query: &str, index: &[PathBuf], root: &Path| -> Vec<String> {
        if query.is_empty() {
            return index
                .iter()
                .take(20)
                .map(|p| {
                    p.strip_prefix(root)
                        .unwrap_or(p)
                        .display()
                        .to_string()
                })
                .collect();
        }
        keplr_core::search::fuzzy_paths(index, query, 20)
            .into_iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap_or(&p)
                    .display()
                    .to_string()
            })
            .collect()
    };

    loop {
        let (_, rows) = terminal::size().unwrap_or((100, 30));
        let height = rows.saturating_sub(3) as usize;
        if cursor.0 < top {
            top = cursor.0;
        }
        if cursor.0 >= top + height.max(1) {
            top = cursor.0 - height.max(1) + 1;
        }
        let max_col = doc.line_chars(cursor.0);
        if cursor.1 > max_col {
            cursor.1 = max_col;
        }
        draw(
            &doc,
            &file_label,
            &branch,
            &lang,
            cursor,
            top,
            dirty,
            &status,
            palette_open,
            &palette_query,
            &palette_hits,
            palette_sel,
        )?;
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('s') => {
                    let content = doc.content();
                    match keplr_core::save_buffer(&ws, &full, &content) {
                        Ok(r) => {
                            dirty = false;
                            quit_armed = false;
                            status = format!(
                                "saved {} hash={} cas={} git={}",
                                r.bytes,
                                &r.hash[..12.min(r.hash.len())],
                                r.cas_stored,
                                r.git_committed
                            );
                        }
                        Err(e) => {
                            status = format!("save failed: {e:#}");
                        }
                    }
                    continue;
                }
                KeyCode::Char('p') => {
                    palette_open = !palette_open;
                    if palette_open {
                        index =
                            keplr_core::Workspace::new(root.clone()).walk_files(20_000);
                        let paths: Vec<PathBuf> =
                            index.iter().map(|e| e.path.clone()).collect();
                        index = paths
                            .into_iter()
                            .map(|p| {
                                let _ = e;
                                p
                            })
                            .collect();
                        palette_query.clear();
                        palette_sel = 0;
                        let all: Vec<PathBuf> = index.clone();
                        palette_hits = refresh_palette("", &all, &root);
                    }
                    continue;
                }
                KeyCode::Char('q') | KeyCode::Char('c') => {
                    if dirty && !quit_armed {
                        quit_armed = true;
                        status = String::from("unsaved changes — press ctrl+q again");
                        continue;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        quit_armed = false;
        if palette_open {
            match key.code {
                KeyCode::Esc => {
                    palette_open = false;
                }
                KeyCode::Enter => {
                    if let Some(hit) = palette_hits.get(palette_sel).cloned() {
                        let next = root.join(&hit);
                        doc = Doc::load(&next);
                        cursor = (0, 0);
                        top = 0;
                        dirty = false;
                        status = format!("opened {hit}");
                    }
                    palette_open = false;
                    palette_query.clear();
                }
                KeyCode::Backspace => {
                    palette_query.pop();
                    let all = index.clone();
                    palette_hits = refresh_palette(&palette_query, &all, &root);
                    palette_sel = 0;
                }
                KeyCode::Up => {
                    palette_sel = palette_sel.saturating_sub(1);
                }
                KeyCode::Down => {
                    palette_sel = (palette_sel + 1).min(palette_hits.len().saturating_sub(1));
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    palette_query.push(c);
                    let all = index.clone();
                    palette_hits = refresh_palette(&palette_query, &all, &root);
                    palette_sel = 0;
                }
                _ => {}
            }
            continue;
        }
        match key.code {
            KeyCode::Left => {
                if cursor.1 > 0 {
                    cursor.1 -= 1;
                } else if cursor.0 > 0 {
                    cursor.0 -= 1;
                    cursor.1 = doc.line_chars(cursor.0);
                }
            }
            KeyCode::Right => {
                if cursor.1 < doc.line_chars(cursor.0) {
                    cursor.1 += 1;
                } else if cursor.0 + 1 < doc.lines.len() {
                    cursor.0 += 1;
                    cursor.1 = 0;
                }
            }
            KeyCode::Up => {
                cursor.0 = cursor.0.saturating_sub(1);
            }
            KeyCode::Down => {
                cursor.0 = (cursor.0 + 1).min(doc.lines.len().saturating_sub(1));
            }
            KeyCode::Home => {
                cursor.1 = 0;
            }
            KeyCode::End => {
                cursor.1 = doc.line_chars(cursor.0);
            }
            KeyCode::PageUp => {
                cursor.0 = cursor.0.saturating_sub(20);
            }
            KeyCode::PageDown => {
                cursor.0 = (cursor.0 + 20).min(doc.lines.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                let byte = doc.byte_col(cursor.0, cursor.1);
                let tail = doc.lines[cursor.0][byte..].to_string();
                doc.lines[cursor.0].truncate(byte);
                doc.lines.insert(cursor.0 + 1, tail);
                cursor.0 += 1;
                cursor.1 = 0;
                dirty = true;
            }
            KeyCode::Backspace => {
                if cursor.1 > 0 {
                    let byte = doc.byte_col(cursor.0, cursor.1);
                    let prev = doc.byte_col(cursor.0, cursor.1 - 1);
                    doc.lines[cursor.0].drain(prev..byte);
                    cursor.1 -= 1;
                    dirty = true;
                } else if cursor.0 > 0 {
                    let tail = doc.lines.remove(cursor.0);
                    cursor.0 -= 1;
                    cursor.1 = doc.line_chars(cursor.0);
                    doc.lines[cursor.0].push_str(&tail);
                    dirty = true;
                }
            }
            KeyCode::Delete => {
                let max = doc.line_chars(cursor.0);
                if cursor.1 < max {
                    let byte = doc.byte_col(cursor.0, cursor.1);
                    let next = doc.byte_col(cursor.0, cursor.1 + 1);
                    doc.lines[cursor.0].drain(byte..next);
                    dirty = true;
                } else if cursor.0 + 1 < doc.lines.len() {
                    let tail = doc.lines.remove(cursor.0 + 1);
                    doc.lines[cursor.0].push_str(&tail);
                    dirty = true;
                }
            }
            KeyCode::Tab => {
                let byte = doc.byte_col(cursor.0, cursor.1);
                doc.lines[cursor.0].insert_str(byte, "  ");
                cursor.1 += 2;
                dirty = true;
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                let byte = doc.byte_col(cursor.0, cursor.1);
                doc.lines[cursor.0].insert(byte, c);
                cursor.1 += 1;
                dirty = true;
            }
            KeyCode::Esc => {}
            _ => {}
        }
        status = format!(
            "{}:{} {}",
            cursor.0 + 1,
            cursor.1 + 1,
            if dirty { "modified" } else { "" }
        );
    }
}
```

Fix the palette-index wart before writing: the `index` variable dance above (`let paths… index = paths.into_iter().map(|p| { let _ = e; p }).collect()`) references an undefined `e` — replace with a clean version: keep `entries: Vec<FileEntry>` from walk, derive `index: Vec<PathBuf>` once:

```rust
                KeyCode::Char('p') => {
                    palette_open = !palette_open;
                    if palette_open {
                        index = keplr_core::Workspace::new(root.clone())
                            .walk_files(20_000)
                            .into_iter()
                            .map(|e| e.path)
                            .collect();
                        palette_query.clear();
                        palette_sel = 0;
                        palette_hits = refresh_palette("", &index, &root);
                    }
                    continue;
                }
```

(The implementer: use this clean block, not the `let _ = e;` draft above.)

Cursor column vs wide chars/tabs is approximate (char count) — documented behavior, matches the ANSI fallback renderer.

- [ ] **Step 3: Wire the command.** In `main.rs` add `mod tui;` at the top, add the variant (after `Open`):

```rust
    Edit {
        file: PathBuf,
    },
```

And the arm (after the `Open` arm):

```rust
        Cmd::Edit { file } => {
            tui::edit_file(cli.root, file)?;
        }
```

- [ ] **Step 4: Commit locally**

```bash
git add Cargo.toml crates/keplr-cli/Cargo.toml crates/keplr-cli/src/tui.rs crates/keplr-cli/src/main.rs
git commit -m "feat(tui): interactive terminal editor on the save pipeline"
```

---

## Part F5: Native GPU shell (feature-gated)

### Task F5-1: `gpu` feature + shell module

**Files:**
- Modify: `Cargo.toml` (`crossterm`, `winit`, `wgpu`, `pollster` in workspace deps)
- Modify: `crates/keplr-render/Cargo.toml` (`[features] gpu`, optional deps)
- Create: `crates/keplr-render/src/gpu.rs`
- Modify: `crates/keplr-render/src/lib.rs` (`pub mod gpu` under cfg)

**Interfaces:**
- Consumes: `SceneSpec`, `build_scene`, `Theme::zed_dark`, `branch_for`
- Produces: `pub fn run_desktop(root: PathBuf, open: Option<PathBuf>, query: String) -> anyhow::Result<()>` (cfg `gpu` only)

- [ ] **Step 1: Workspace deps.** Append to `[workspace.dependencies]` in `Cargo.toml`:

```toml
crossterm = "0.28"
winit = "0.30"
wgpu = "22"
pollster = "0.3"
```

- [ ] **Step 2: Render Cargo.** Append:

```toml
[features]
gpu = ["dep:winit", "dep:wgpu", "dep:pollster"]

[dependencies]
# ... existing ...
winit = { workspace = true, optional = true }
wgpu = { workspace = true, optional = true, features = ["wgsl", "winit"] }
pollster = { workspace = true, optional = true }
```

(Read the file first and append exactly; keep existing lines byte-identical. If cargo reports the `winit` feature does not exist on `wgpu 22`, drop `, "winit"` and retry.)

- [ ] **Step 3: `mod` line.** In `crates/keplr-render/src/lib.rs` after `pub mod search;`-style lines (check exact lines first — the file starts with `pub mod buffer; pub mod search;`):

```rust
#[cfg(feature = "gpu")]
pub mod gpu;
```

- [ ] **Step 4: Create `crates/keplr-render/src/gpu.rs`** with the full module below. It paints the Zed dock layout as theme-colored rectangles (no text rasterization — that needs a font stack and is the documented next layer), tracks FPS in the title, rebuilds the scene on F5, exits on Esc/close, and returns `Err` (for the CLI software fallback) when no GPU/surface is available:

```rust
use super::{branch_for, Scene, SceneSpec, Theme};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) col: vec4<f32>,
};
@vertex
fn vs(@location(0) p: vec2<f32>, @location(1) c: vec4<f32>) -> VsOut {
    var o: VsOut;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.col = c;
    return o;
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return in.col;
}
"#;

fn parse_hex(hex: &str) -> [f32; 3] {
    let h = hex.trim_start_matches('#');
    let n = u32::from_str_radix(h, 16).unwrap_or(0x0e1116);
    [
        ((n >> 16) & 0xff) as f32 / 255.0,
        ((n >> 8) & 0xff) as f32 / 255.0,
        (n & 0xff) as f32 / 255.0,
    ]
}

fn push_quad(v: &mut Vec<f32>, x: f32, y: f32, w: f32, h: f32, c: [f32; 3]) {
    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y), (x + w, y + h), (x, y + h)];
    for (px, py) in corners {
        v.push(px);
        v.push(py);
        v.push(c[0]);
        v.push(c[1]);
        v.push(c[2]);
        v.push(1.0);
    }
}

fn layout_quads(_scene: &Scene, w: u32, h: u32) -> Vec<f32> {
    let theme = Theme::zed_dark();
    let bg = parse_hex(&theme.bg);
    let surface = parse_hex(&theme.surface);
    let accent = parse_hex(&theme.accent);
    let w = w.max(1) as f32;
    let h = h.max(1) as f32;
    let nx = |px: f32| px / w * 2.0 - 1.0;
    let ny = |py: f32| 1.0 - py / h * 2.0;
    let mut v = Vec::new();
    let mut quad = |px: f32, py: f32, pw: f32, ph: f32, c: [f32; 3]| {
        let x0 = nx(px);
        let x1 = nx(px + pw);
        let y0 = ny(py);
        let y1 = ny(py + ph);
        push_quad(&mut v, x0, y0, x1 - x0, y1 - y0, c);
    };
    quad(0.0, 0.0, w, h, bg);
    quad(0.0, 0.0, w, 30.0, surface);
    quad(0.0, 30.0, w, 2.0, accent);
    quad(0.0, 32.0, w * 0.22, h - 58.0, surface);
    quad(w * 0.82, 32.0, w * 0.18, h - 58.0, surface);
    quad(0.0, h - 26.0, w, 26.0, surface);
    quad(0.0, h - 120.0, w, 94.0, surface);
    v
}

fn f32_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vbuf: wgpu::Buffer,
    vcap: usize,
    size: (u32, u32),
}

impl Gpu {
    async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| anyhow::anyhow!("no surface: {e:?}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("no GPU adapter found"))?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("no GPU device: {e:?}"))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let size = (size.width.max(1), size.height.max(1));
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0,
            height: size.1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("keplr"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 65536,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vbuf,
            vcap: 65536,
            size,
        })
    }

    fn upload(&mut self, verts: &[f32]) {
        let bytes = f32_to_bytes(verts);
        if bytes.len() > self.vcap {
            self.vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vcap = bytes.len();
        }
        self.queue.write_buffer(&self.vbuf, 0, &bytes);
    }

    fn frame(&mut self, clear: wgpu::Color, verts: usize) -> anyhow::Result<()> {
        let texture = self
            .surface
            .get_current_texture()
            .map_err(|e| anyhow::anyhow!("surface lost: {e:?}"))?;
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            pass.draw(0..verts as u32, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        texture.present();
        Ok(())
    }
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    init_err: Option<String>,
    root: PathBuf,
    open: Option<PathBuf>,
    query: String,
    scene: Option<Scene>,
    dirty: bool,
    frames: u32,
    since: Instant,
}

impl App {
    fn rebuild(&mut self) {
        let spec = SceneSpec {
            root: &self.root,
            open_file: self.open.as_deref(),
            query: self.query.as_str(),
            palette_query: None,
            palette_mode: "files",
            search_query: None,
            left_tab: "project",
            right_tab: "symbols",
            bottom_tab: "terminal",
            width: 100,
        };
        self.scene = Some(super::build_scene(&spec));
        self.dirty = true;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = match event_loop.create_window(
            WindowAttributes::default().with_title("keplr"),
        ) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.init_err = Some(format!("no window: {e:?}"));
                event_loop.exit();
                return;
            }
        };
        match pollster::block_on(Gpu::new(window.clone())) {
            Ok(g) => {
                self.window = Some(window);
                self.gpu = Some(g);
                self.rebuild();
            }
            Err(e) => {
                self.init_err = Some(format!("{e:#}"));
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(gpu), Some(window)) = (self.gpu.as_mut(), self.window.as_ref())
                {
                    let w = size.width.max(1);
                    let h = size.height.max(1);
                    gpu.config.width = w;
                    gpu.config.height = h;
                    gpu.size = (w, h);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                    let _ = window;
                    self.dirty = true;
                }
            }
            WindowEvent::RedrawRequested => {
                let mut failed: Option<String> = None;
                if let (Some(gpu), Some(window)) =
                    (self.gpu.as_mut(), self.window.as_ref())
                {
                    if self.scene.is_none() || self.dirty {
                        self.rebuild();
                    }
                    if let Some(scene) = &self.scene {
                        let quads = layout_quads(scene, gpu.size.0, gpu.size.1);
                        let n = quads.len() / 6;
                        gpu.upload(&quads);
                        let bg = parse_hex(&Theme::zed_dark().bg);
                        let clear = wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: 1.0,
                        };
                        if let Err(e) = gpu.frame(clear, n) {
                            failed = Some(format!("{e:#}"));
                        }
                    }
                    self.frames += 1;
                    if self.since.elapsed().as_millis() >= 500 {
                        let fps =
                            self.frames as f64 / self.since.elapsed().as_secs_f64();
                        let label = self
                            .open
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| String::from("(no file)"));
                        window.set_title(&format!(
                            "keplr — {label} — {} — {fps:.0}fps",
                            branch_for(&self.root)
                        ));
                        self.frames = 0;
                        self.since = Instant::now();
                    }
                    self.dirty = false;
                    window.request_redraw();
                }
                if let Some(e) = failed {
                    eprintln!("keplr: gpu frame failed: {e}");
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Released {
                    return;
                }
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::F5) => {
                        self.rebuild();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

pub fn run_desktop(
    root: PathBuf,
    open: Option<PathBuf>,
    query: String,
) -> anyhow::Result<()> {
    let event_loop =
        EventLoop::new().map_err(|e| anyhow::anyhow!("no event loop: {e:?}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        window: None,
        gpu: None,
        init_err: None,
        root,
        open,
        query,
        scene: None,
        dirty: true,
        frames: 0,
        since: Instant::now(),
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!("event loop failed: {e:?}"))?;
    if let Some(e) = app.init_err {
        return Err(anyhow::anyhow!("{e}"));
    }
    Ok(())
}
```

Notes for the implementer: `use wgpu::util::DeviceExt;` is imported but unused if `create_buffer_init` is never called — remove that import (the code uses `create_buffer` + `write_buffer`). The snippet above already omits it. `DeviceDescriptor.memory_hints` and `RenderPassColorAttachment.depth_slice`: if the pinned `wgpu 22` API differs, CI will say exactly which field — adjust minimally. `WindowId` import is used in the `window_event` signature.

- [ ] **Step 5: Commit locally**

```bash
git add Cargo.toml crates/keplr-render/Cargo.toml crates/keplr-render/src/gpu.rs crates/keplr-render/src/lib.rs
git commit -m "feat(gpu): feature-gated native shell with layout paint"
```

### Task F5-2: `keplr desktop` + CI gpu job + docs

**Files:**
- Modify: `crates/keplr-cli/Cargo.toml` (`desktop` feature)
- Modify: `crates/keplr-cli/src/main.rs` (`Desktop` variant + arm, both cfg'd)
- Modify: `.github/workflows/ci.yml` (new `gpu-check` job)
- Modify: `README.md` (Plan F usage)

- [ ] **Step 1: CLI feature.** In `crates/keplr-cli/Cargo.toml` append:

```toml
[features]
desktop = ["keplr-render/gpu"]
```

- [ ] **Step 2: CLI variant** (after the `Scene` variant):

```rust
    #[cfg(feature = "desktop")]
    Desktop {
        #[arg(long)]
        open: Option<PathBuf>,
        #[arg(long, default_value = "")]
        query: String,
    },
```

- [ ] **Step 3: CLI arm** (after the `Scene` arm, before the closing `}` of the match):

```rust
        #[cfg(feature = "desktop")]
        Cmd::Desktop { open, query } => {
            match keplr_render::gpu::run_desktop(cli.root.clone(), open, query) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("keplr: gpu unavailable ({e:#}); software fallback");
                    let mut ui = keplr_ui::UiState::new(cli.root.clone());
                    if let Some(path) = open.clone() {
                        ui.open_file(path);
                    }
                    if !query.is_empty() {
                        ui.palette_query = query.clone();
                    }
                    let scene = ui.to_scene(100);
                    print!(
                        "{}",
                        keplr_render::PaintBackend::paint(
                            &keplr_render::AnsiBackend,
                            &scene,
                            100
                        )
                    );
                }
            }
        }
```

Wait — `open`/`query` are moved into `run_desktop` then used in the fallback. Fix: clone at the call: `keplr_render::gpu::run_desktop(cli.root.clone(), open.clone(), query.clone())`. (The implementer: use the cloned call.)

- [ ] **Step 4: CI job.** Read `.github/workflows/ci.yml` first, then append a second job (existing job byte-identical):

```yaml
  gpu-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check -p keplr-render --features gpu
      - run: cargo check -p keplr-cli --features desktop
```

- [ ] **Step 5: README** — add the Plan F link line next to the Plan E link, and append:

```markdown
## Use (Plan F save/bench/auth/tui/gpu, production)

echo 'hello keplr' | cargo run -p keplr-cli -- --root . save notes.txt --stdin
cargo run -p keplr-cli -- --root . save notes.txt --content "hello" --task lint
curl -X POST 127.0.0.1:7137/save -H 'Content-Type: application/json' -d '{"path":"notes.txt","content":"hi"}'
cargo run -p keplr-cli -- --root /tmp/bench bench --files 2000 --lines 40
cargo run -p keplr-cli -- --root . token --save
cargo run -p keplr-cli -- --root . serve --port 7137  # KEPLR_TOKEN=... or --token ...
cargo run -p keplr-cli -- --root . edit Cargo.toml
cargo run -p keplr-cli --features desktop -- --root . desktop --open Cargo.toml  # native window; falls back to ANSI without GPU
```

Plus an honest scope note under it:

```markdown
WASM browser parity is deferred: `notify`/`ignore` are not wasm-safe, so the workspace needs dep surgery first. The browser contract is already live — `Scene` serde JSON over `keplr serve` (see `/scene`).
```

- [ ] **Step 6: Commit locally, push, watch CI**

```bash
git add crates/keplr-cli/Cargo.toml crates/keplr-cli/src/main.rs .github/workflows/ci.yml README.md
git commit -m "feat(desktop): gpu-gated command with software fallback plus ci check"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: default job green + `gpu-check` green. On failure: `gh run view <id> --log-failed`, fix (likely `wgpu` field drift or the `winit` feature name), push again.

---

## Self-review (run before handoff)

1. Spec coverage: §8 save flow yes (F1 write→CAS→index→git→optional task); §10 benches yes (F2 synth + p50/p95 + CAS rate + skip rate); §5 hub auth yes (F3 bearer gate; Tailscale/WireGuard remains an operator transport choice); §4 editor/input yes at terminal level (F4 keystroke→rope→repaint→save; IME/touch need the OS shell); §6 live logs yes at report granularity (streaming per-keystroke task output is out of scope); §2 native shell yes behind `gpu` (window+surface+layout paint+resilience fallback; GPU text rasterization is the documented next layer); WASM explicitly deferred with reason.
2. Placeholder scan: no TBD/TODO/placeholder/unimplemented; `desktop` without GPU prints the real software UI; `save` without git reports honestly; `bench` measures instead of estimating.
3. Type consistency: `SaveReport/save_buffer`, `synth_tree/percentile_ns`, `new_token/resolve_token/serve_with_token`, `tui::edit_file`, `gpu::run_desktop` spelled identically at every definition and call site; `serve(root, port)` signature unchanged.
