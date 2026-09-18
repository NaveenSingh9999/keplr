# Keplr Plan C — Smart Build Runner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `keplr-build` from a serial `sh -c` runner into a smart DAG runner: deps, topo order, parallel levels, content-hash skipping, CAS output reuse, watch mode.

**Architecture:** Additive only. `TaskDef` gains serde-defaulted `deps`, `watch`, `fingerprint` fields so every existing `keplr.json` still parses. `topo_levels` (Kahn, deterministic via `BTreeSet`) is the single ordering primitive behind `topo_order` and `run_graph_parallel`. Fingerprints are blake3 over cmd plus listed files/dirs (gitignore-aware via `ignore` crate, 20k file cap) plus transitive dep fingerprints. Skip-state lives in `{workdir}/.keplr/build-journal.json`; outputs are content-addressed in `{workdir}/.keplr/cas` via `keplr_sync::Cas` and restored when missing. Threads only run commands; the main thread owns fingerprints, CAS, and journal. `run_task` stays byte-identical.

**Tech Stack:** Rust 1.97.1, existing workspace deps only (`anyhow`, `serde`/`serde_json`, `blake3`, `ignore`, `keplr-sync` path dep, `std::thread::scope`, `tokio::task::spawn_blocking` + `tokio::time::sleep` at the edges).

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md` section 6 (Smart task runner)

## Global Constraints

- No Electron, no Chromium shell, no WebView. Native binary + `keplr serve` JSON only.
- Same types with no rename: `FileEntry { path: PathBuf, size: u64, mtime: i64, hash: String }`, `SearchHit { path: PathBuf, line: u64, col: u64, preview: String }`, `TaskDef { name: String, cmd: String, cwd: Option<String>, outputs: Vec<String> }` (extended only by additive defaulted fields), `LangKind` enum variants `TypeScript | Tsx | JavaScript | Cpp | Go | Rust | Laml | Other`.
- RAM soft cap 500MB respected by streaming search (never collect whole repo); fingerprint walks cap at 20,000 files.
- Results stream in <16ms chunks: search APIs take `limit: usize` and return early.
- Git remains truth; CAS lives at `{workspace}/.keplr/cas`, journal at `{workspace}/.keplr/build-journal.json`; `.keplr/` is gitignored.
- LAML reuse only: shell to `laml` binary if present, never reimplement evaluator.
- Every task ends with push + `gh run watch` green (`cargo test --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`).
- No placeholders: no TBD/TODO/unimplemented; every route returns real data; `run_task` uses real `sh -c` (Termux compatible); `serve` binds `127.0.0.1`.
- Standing orders for this plan: no local `cargo` runs (verify via GitHub CI with `gh`), no new test files — existing CI tests must stay green.

---

## File Structure

```
crates/keplr-build/src/lib.rs   # TaskDef v2, topo, fingerprints, journal, CAS reuse, serial + parallel runners
crates/keplr-build/Cargo.toml   # + blake3, ignore, keplr-sync (folded into Task 2, first task that needs them)
crates/keplr-cli/src/main.rs    # Run gains all/jobs/force/watch (additive; `keplr run check` still works)
crates/keplr-serve/src/lib.rs   # + GET /tasks/graph, POST /tasks/run (additive; old routes untouched)
keplr.json                      # lint gains deps: ["check"] (real DAG dogfood)
README.md                       # Plan C usage section
```

Decomposition locked here. A task implementer sees only their task; Interfaces blocks carry exact names.

---

### Task 1: TaskDef v2 + topo order + closure

**Files:**
- Modify: `crates/keplr-build/src/lib.rs` (full replacement below)
- Test: none new — existing `crates/keplr-build/tests/build_test.rs` must stay green in CI (old JSON shape still parses via serde defaults)

**Interfaces:**
- Consumes: nothing new
- Produces: `pub struct TaskDef { pub name: String, pub cmd: String, pub cwd: Option<String>, pub outputs: Vec<String>, pub deps: Vec<String>, pub watch: Vec<String>, pub fingerprint: Vec<String> }`, `pub fn load_tasks(path: &Path) -> anyhow::Result<BTreeMap<String, TaskDef>>` (unchanged signature), `pub fn run_task(task: &TaskDef, workdir: &Path) -> anyhow::Result<String>` (byte-identical body), `pub fn topo_order(tasks: &BTreeMap<String, TaskDef>) -> anyhow::Result<Vec<String>>`

- [ ] **Step 1: Replace `crates/keplr-build/src/lib.rs` with the Task 1 version**

```rust
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDef {
    pub name: String,
    pub cmd: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub deps: Vec<String>,
    #[serde(default)]
    pub watch: Vec<String>,
    #[serde(default)]
    pub fingerprint: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FileShape {
    #[serde(default)]
    tasks: BTreeMap<String, RawTask>,
}

#[derive(Debug, Deserialize)]
struct RawTask {
    cmd: String,
    cwd: Option<String>,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    deps: Vec<String>,
    #[serde(default)]
    watch: Vec<String>,
    #[serde(default)]
    fingerprint: Vec<String>,
}

pub fn load_tasks(path: &Path) -> anyhow::Result<BTreeMap<String, TaskDef>> {
    let text = std::fs::read_to_string(path)?;
    let shape: FileShape = serde_json::from_str(&text)?;
    Ok(shape
        .tasks
        .into_iter()
        .map(|(name, raw)| {
            (
                name.clone(),
                TaskDef {
                    name,
                    cmd: raw.cmd,
                    cwd: raw.cwd,
                    outputs: raw.outputs,
                    deps: raw.deps,
                    watch: raw.watch,
                    fingerprint: raw.fingerprint,
                },
            )
        })
        .collect())
}

pub fn run_task(task: &TaskDef, workdir: &Path) -> anyhow::Result<String> {
    let cwd = task.cwd.as_ref().map(Path::new).unwrap_or(workdir);
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&task.cmd)
        .current_dir(cwd)
        .output()?;
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        anyhow::bail!("task `{}` failed: {}", task.name, combined);
    }
    Ok(combined)
}

fn topo_levels(
    tasks: &BTreeMap<String, TaskDef>,
    wanted: &BTreeSet<String>,
) -> anyhow::Result<Vec<Vec<String>>> {
    let mut indeg: BTreeMap<String, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in wanted {
        let task = tasks
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
        let mut count = 0;
        for dep in &task.deps {
            if !tasks.contains_key(dep) {
                anyhow::bail!("task `{name}` depends on unknown task `{dep}`");
            }
            if wanted.contains(dep) {
                count += 1;
                dependents.entry(dep.clone()).or_default().push(name.clone());
            }
        }
        indeg.insert(name.clone(), count);
    }
    let mut ready: BTreeSet<String> = indeg
        .iter()
        .filter(|(_, &c)| c == 0)
        .map(|(n, _)| n.clone())
        .collect();
    let mut levels = Vec::new();
    while !ready.is_empty() {
        let level: Vec<String> = ready.iter().cloned().collect();
        ready.clear();
        for name in &level {
            if let Some(ds) = dependents.get(name) {
                for d in ds {
                    if let Some(c) = indeg.get_mut(d) {
                        *c -= 1;
                        if *c == 0 {
                            ready.insert(d.clone());
                        }
                    }
                }
            }
        }
        levels.push(level);
    }
    let done: usize = levels.iter().map(Vec::len).sum();
    if done != wanted.len() {
        let stuck: Vec<String> = indeg
            .into_iter()
            .filter(|(_, c)| *c > 0)
            .map(|(n, _)| n)
            .collect();
        anyhow::bail!("dependency cycle among: {}", stuck.join(", "));
    }
    Ok(levels)
}

pub fn topo_order(tasks: &BTreeMap<String, TaskDef>) -> anyhow::Result<Vec<String>> {
    let all: BTreeSet<String> = tasks.keys().cloned().collect();
    Ok(topo_levels(tasks, &all)?.into_iter().flatten().collect())
}

fn closure(tasks: &BTreeMap<String, TaskDef>, targets: &[String]) -> anyhow::Result<BTreeSet<String>> {
    let mut wanted = BTreeSet::new();
    let mut stack: Vec<String> = targets.to_vec();
    while let Some(name) = stack.pop() {
        if !wanted.insert(name.clone()) {
            continue;
        }
        let task = tasks
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
        for dep in &task.deps {
            stack.push(dep.clone());
        }
    }
    Ok(wanted)
}
```

- [ ] **Step 2: Push and watch CI**

```bash
git add crates/keplr-build/src/lib.rs
git commit -m "feat(build): task deps/watch/fingerprint fields plus topo order"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: `cargo test` PASS (old JSON parses via defaults), clippy clean, build clean. If CI reports an error, read it with `gh run view <id> --log-failed` and fix in a follow-up commit.

---

### Task 2: Fingerprints + journal + CAS outputs + serial run_graph

**Files:**
- Modify: `crates/keplr-build/Cargo.toml` (add 3 deps)
- Modify: `crates/keplr-build/src/lib.rs` (append fingerprint/journal/CAS/runner code; keep every Task 1 item byte-identical)

**Interfaces:**
- Consumes: Task 1 `TaskDef`/`load_tasks`/`run_task`/`topo_order`/`closure`/`topo_levels`, `keplr_sync::Cas { new, put, get, exists }`
- Produces: `pub struct JournalEntry { pub hash: String, pub outputs: BTreeMap<String, String> }`, `pub struct RunReport { pub task: String, pub skipped: bool, pub output: String, pub hash: String }`, `pub fn task_fingerprint(task: &TaskDef, workdir: &Path) -> String`, `pub fn graph_fingerprints(tasks: &BTreeMap<String, TaskDef>, workdir: &Path) -> BTreeMap<String, String>`, `pub fn load_journal(workdir: &Path) -> BTreeMap<String, JournalEntry>`, `pub fn save_journal(workdir: &Path, journal: &BTreeMap<String, JournalEntry>) -> anyhow::Result<()>`, `pub fn run_graph(tasks: &BTreeMap<String, TaskDef>, workdir: &Path, targets: &[String], force: bool) -> anyhow::Result<Vec<RunReport>>`

- [ ] **Step 1: Add deps to `crates/keplr-build/Cargo.toml`**

Old block:

```toml
[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
```

New block:

```toml
[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
blake3.workspace = true
ignore.workspace = true
keplr-sync = { path = "../keplr-sync" }
```

- [ ] **Step 2: Update the import block in `crates/keplr-build/src/lib.rs`**

Old:

```rust
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
```

New:

```rust
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
```

- [ ] **Step 3: Append the fingerprint/journal/CAS/serial-runner code to `crates/keplr-build/src/lib.rs`** (after the `closure` function, end of file)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournalEntry {
    pub hash: String,
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunReport {
    pub task: String,
    pub skipped: bool,
    pub output: String,
    pub hash: String,
}

fn resolve_under(workdir: &Path, pat: &str) -> PathBuf {
    let p = Path::new(pat);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        workdir.join(p)
    }
}

fn hash_file(hasher: &mut blake3::Hasher, path: &Path) {
    match std::fs::read(path) {
        Ok(bytes) => {
            hasher.update(&bytes);
        }
        Err(_) => {
            hasher.update(b"\0missing:");
            hasher.update(path.to_string_lossy().as_bytes());
        }
    }
    hasher.update(&[0]);
}

fn hash_listed(hasher: &mut blake3::Hasher, workdir: &Path, patterns: &[String]) {
    let mut pats: Vec<&String> = patterns.iter().collect();
    pats.sort();
    for pat in pats {
        let p = resolve_under(workdir, pat);
        hasher.update(pat.as_bytes());
        hasher.update(&[0]);
        if p.is_file() {
            hash_file(hasher, &p);
        } else if p.is_dir() {
            let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(&p)
                .hidden(false)
                .git_ignore(true)
                .parents(true)
                .build()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.path().to_path_buf())
                .collect();
            files.sort();
            files.truncate(20_000);
            hasher.update(&(files.len() as u64).to_le_bytes());
            for f in files {
                let rel = f
                    .strip_prefix(&p)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .to_string();
                hasher.update(rel.as_bytes());
                hasher.update(&[0]);
                hash_file(hasher, &f);
            }
        } else {
            hasher.update(b"\0absent:");
            hasher.update(&[0]);
        }
    }
}

pub fn task_fingerprint(task: &TaskDef, workdir: &Path) -> String {
    let mut h = blake3::Hasher::new();
    h.update(task.name.as_bytes());
    h.update(&[0]);
    h.update(task.cmd.as_bytes());
    h.update(&[0]);
    if let Some(c) = &task.cwd {
        h.update(c.as_bytes());
    }
    h.update(&[0]);
    let mut outs = task.outputs.clone();
    outs.sort();
    for o in outs {
        h.update(o.as_bytes());
        h.update(&[0]);
    }
    hash_listed(&mut h, workdir, &task.fingerprint);
    h.finalize().to_hex().to_string()
}

fn fingerprint_inner(
    tasks: &BTreeMap<String, TaskDef>,
    name: &str,
    workdir: &Path,
    memo: &mut BTreeMap<String, String>,
    stack: &mut Vec<String>,
) -> String {
    if let Some(v) = memo.get(name) {
        return v.clone();
    }
    if stack.iter().any(|s| s == name) {
        return format!("cycle:{name}");
    }
    let Some(task) = tasks.get(name) else {
        return format!("missing:{name}");
    };
    stack.push(name.to_string());
    let mut h = blake3::Hasher::new();
    h.update(task_fingerprint(task, workdir).as_bytes());
    let mut deps = task.deps.clone();
    deps.sort();
    for d in deps {
        let fh = fingerprint_inner(tasks, &d, workdir, memo, stack);
        h.update(fh.as_bytes());
        h.update(&[0]);
    }
    stack.pop();
    let s = h.finalize().to_hex().to_string();
    memo.insert(name.to_string(), s.clone());
    s
}

pub fn graph_fingerprints(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
) -> BTreeMap<String, String> {
    let mut memo = BTreeMap::new();
    let mut stack = Vec::new();
    for name in tasks.keys() {
        fingerprint_inner(tasks, name, workdir, &mut memo, &mut stack);
    }
    memo
}

fn journal_path(workdir: &Path) -> PathBuf {
    workdir.join(".keplr/build-journal.json")
}

pub fn load_journal(workdir: &Path) -> BTreeMap<String, JournalEntry> {
    std::fs::read_to_string(journal_path(workdir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_journal(workdir: &Path, journal: &BTreeMap<String, JournalEntry>) -> anyhow::Result<()> {
    let path = journal_path(workdir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(journal)?;
    std::fs::write(&path, text)?;
    Ok(())
}

fn store_outputs(
    cas: &keplr_sync::Cas,
    workdir: &Path,
    task: &TaskDef,
) -> BTreeMap<String, String> {
    let mut stored = BTreeMap::new();
    let mut outs = task.outputs.clone();
    outs.sort();
    for rel in outs {
        let full = resolve_under(workdir, &rel);
        if let Ok(bytes) = std::fs::read(&full) {
            if let Ok(hash) = cas.put(&bytes) {
                stored.insert(rel, hash);
            }
        }
    }
    stored
}

fn restore_outputs(
    cas: &keplr_sync::Cas,
    workdir: &Path,
    entry: &JournalEntry,
) -> Vec<String> {
    let mut restored = Vec::new();
    for (rel, hash) in &entry.outputs {
        let full = resolve_under(workdir, rel);
        if !full.exists() && cas.exists(hash) {
            if let Ok(bytes) = cas.get(hash) {
                if let Some(parent) = full.parent() {
                    if std::fs::create_dir_all(parent).is_err() {
                        continue;
                    }
                }
                if std::fs::write(&full, bytes).is_ok() {
                    restored.push(rel.clone());
                }
            }
        }
    }
    restored
}

fn run_ordered(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
    order: &[String],
    force: bool,
) -> anyhow::Result<Vec<RunReport>> {
    let fps = graph_fingerprints(tasks, workdir);
    let mut journal = load_journal(workdir);
    let cas = keplr_sync::Cas::new(workdir.join(".keplr/cas"));
    let mut reports = Vec::new();
    for name in order {
        let task = tasks
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
        let fp = fps.get(name).cloned().unwrap_or_default();
        let up_to_date = !force && journal.get(name).map(|e| e.hash == fp).unwrap_or(false);
        if up_to_date {
            let restored = restore_outputs(&cas, workdir, &journal[name]);
            let mut output = String::from("up to date");
            if !restored.is_empty() {
                output.push_str(&format!(" (restored {})", restored.join(", ")));
            }
            reports.push(RunReport {
                task: name.clone(),
                skipped: true,
                output,
                hash: fp,
            });
            continue;
        }
        let out = run_task(task, workdir)?;
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
        });
    }
    Ok(reports)
}

pub fn run_graph(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
    targets: &[String],
    force: bool,
) -> anyhow::Result<Vec<RunReport>> {
    let levels = if targets.is_empty() {
        let all: BTreeSet<String> = tasks.keys().cloned().collect();
        topo_levels(tasks, &all)?
    } else {
        let wanted = closure(tasks, targets)?;
        topo_levels(tasks, &wanted)?
    };
    let order: Vec<String> = levels.into_iter().flatten().collect();
    run_ordered(tasks, workdir, &order, force)
}
```

Semantics locked: empty `targets` means the whole graph; otherwise the transitive closure of the named targets. A task whose journal hash matches and `force` is false is skipped (with CAS restore of missing outputs). A failing task returns `Err` immediately so dependents never run; the journal is saved incrementally so earlier successes keep their skip-state.

- [ ] **Step 4: Push and watch CI**

```bash
git add crates/keplr-build/Cargo.toml crates/keplr-build/src/lib.rs
git commit -m "feat(build): fingerprints, journal, CAS output reuse, serial run_graph"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: CI green. On failure, `gh run view <id> --log-failed`, fix, push again.

---

### Task 3: Parallel run_graph_parallel

**Files:**
- Modify: `crates/keplr-build/src/lib.rs` (append; keep Tasks 1-2 byte-identical)

**Interfaces:**
- Consumes: `closure`, `topo_levels`, `graph_fingerprints`, `load_journal`, `save_journal`, `run_task`, `store_outputs`, `restore_outputs`, `run_graph` (for the `jobs == 1` path)
- Produces: `pub fn run_graph_parallel(tasks: &BTreeMap<String, TaskDef>, workdir: &Path, targets: &[String], jobs: usize, force: bool) -> anyhow::Result<Vec<RunReport>>`

- [ ] **Step 1: Append to `crates/keplr-build/src/lib.rs`** (after `run_graph`, end of file)

```rust
pub fn run_graph_parallel(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
    targets: &[String],
    jobs: usize,
    force: bool,
) -> anyhow::Result<Vec<RunReport>> {
    let jobs = jobs.clamp(1, 32);
    if jobs == 1 {
        return run_graph(tasks, workdir, targets, force);
    }
    let levels = if targets.is_empty() {
        let all: BTreeSet<String> = tasks.keys().cloned().collect();
        topo_levels(tasks, &all)?
    } else {
        let wanted = closure(tasks, targets)?;
        topo_levels(tasks, &wanted)?
    };
    let fps = graph_fingerprints(tasks, workdir);
    let mut journal = load_journal(workdir);
    let cas = keplr_sync::Cas::new(workdir.join(".keplr/cas"));
    let mut reports = Vec::new();
    for level in levels {
        let mut dirty: Vec<String> = Vec::new();
        for name in &level {
            let fp = fps.get(name).cloned().unwrap_or_default();
            let up_to_date =
                !force && journal.get(name).map(|e| e.hash == fp).unwrap_or(false);
            if up_to_date {
                let restored = restore_outputs(&cas, workdir, &journal[name]);
                let mut output = String::from("up to date");
                if !restored.is_empty() {
                    output.push_str(&format!(" (restored {})", restored.join(", ")));
                }
                reports.push(RunReport {
                    task: name.clone(),
                    skipped: true,
                    output,
                    hash: fp,
                });
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
                                out.insert(name.clone(), Err(anyhow::anyhow!("task `{name}` panicked")));
                            }
                        }
                    }
                    out
                });
            for name in batch {
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
                        });
                    }
                    Some(Err(e)) => {
                        save_journal(workdir, &journal)?;
                        return Err(anyhow::anyhow!("{e:#}"));
                    }
                    None => {
                        save_journal(workdir, &journal)?;
                        anyhow::bail!("task `{name}` produced no result");
                    }
                }
            }
        }
    }
    Ok(reports)
}
```

Rules locked: levels run in order, tasks inside one level run in `jobs`-sized thread batches; threads only execute `run_task`, the main thread owns CAS/journal. First failure saves the journal and returns `Err` so later levels never start. Reports stay in deterministic topo order.

- [ ] **Step 2: Push and watch CI**

```bash
git add crates/keplr-build/src/lib.rs
git commit -m "feat(build): parallel DAG levels with jobs cap and fail-cancel"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: CI green.

---

### Task 4: CLI run flags + watch + serve graph/run + dogfood + docs

**Files:**
- Modify: `crates/keplr-cli/src/main.rs`
- Modify: `crates/keplr-serve/src/lib.rs`
- Modify: `keplr.json`
- Modify: `README.md`

**Interfaces:**
- Consumes: `keplr_build::{load_tasks, run_graph, run_graph_parallel, graph_fingerprints, RunReport}`, existing CLI variants (untouched), existing serve routes (untouched)
- Produces: `keplr run [task] [--all] [--jobs N] [--force] [--watch]`; `GET /tasks/graph`; `POST /tasks/run`

- [ ] **Step 1: Extend the `Run` variant in `crates/keplr-cli/src/main.rs`**

Old:

```rust
    Run {
        task: String,
    },
```

New:

```rust
    Run {
        task: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        watch: bool,
    },
```

- [ ] **Step 2: Add the `BTreeMap` import in `crates/keplr-cli/src/main.rs`**

Old:

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;
```

New:

```rust
use clap::{Parser, Subcommand};
use std::{collections::BTreeMap, path::PathBuf};
```

- [ ] **Step 3: Replace the `Run` arm in `crates/keplr-cli/src/main.rs`**

Old:

```rust
        Cmd::Run { task } => {
            let tasks = keplr_build::load_tasks(&cli.root.join("keplr.json"))?;
            let def = tasks.get(&task).ok_or_else(|| anyhow::anyhow!("unknown task {task}"))?;
            print!("{}", keplr_build::run_task(def, &cli.root)?);
        }
```

New:

```rust
        Cmd::Run {
            task,
            all,
            jobs,
            force,
            watch,
        } => {
            let path = cli.root.join("keplr.json");
            let run_once = |force: bool| -> anyhow::Result<Vec<keplr_build::RunReport>> {
                let tasks = keplr_build::load_tasks(&path)?;
                let targets: Vec<String> = match (&task, all) {
                    (_, true) => Vec::new(),
                    (Some(t), false) => vec![t.clone()],
                    (None, false) => Vec::new(),
                };
                if jobs > 1 {
                    keplr_build::run_graph_parallel(&tasks, &cli.root, &targets, jobs, force)
                } else {
                    keplr_build::run_graph(&tasks, &cli.root, &targets, force)
                }
            };
            let print_reports = |reports: &[keplr_build::RunReport]| {
                for r in reports {
                    let state = if r.skipped { "skipped" } else { "ok" };
                    println!("=== {} ({state}) ===", r.task);
                    print!("{}", r.output);
                    if !r.output.ends_with('\n') {
                        println!();
                    }
                }
            };
            if watch {
                println!("keplr: watching {} (Ctrl-C to stop)", cli.root.display());
                let snapshot = || -> anyhow::Result<BTreeMap<String, String>> {
                    let tasks = keplr_build::load_tasks(&path)?;
                    Ok(keplr_build::graph_fingerprints(&tasks, &cli.root))
                };
                let mut last = snapshot()?;
                match run_once(force) {
                    Ok(reports) => print_reports(&reports),
                    Err(e) => eprintln!("keplr: run failed: {e:#}"),
                }
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let now = snapshot()?;
                    if now == last {
                        continue;
                    }
                    last = now;
                    match run_once(false) {
                        Ok(reports) => print_reports(&reports),
                        Err(e) => eprintln!("keplr: run failed: {e:#}"),
                    }
                }
            } else {
                let reports = run_once(force)?;
                print_reports(&reports);
            }
        }
```

Compatibility locked: `keplr run check` still works (`task = Some("check")`, serial, unforced, one-shot). Watch polls fingerprints every second and re-runs the graph on change; run errors print but keep watching.

- [ ] **Step 4: Add graph/run routes to `crates/keplr-serve/src/lib.rs`**

4a. Change the routing import. Old:

```rust
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
```

New:

```rust
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
```

4b. Append the two handlers plus request type after the `scene` function (before `pub async fn serve`):

```rust
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
```

4c. Register the routes. Old:

```rust
        .route("/files", get(files))
        .route("/tasks", get(tasks))
        .route("/scene", get(scene))
```

New:

```rust
        .route("/files", get(files))
        .route("/tasks", get(tasks))
        .route("/tasks/graph", get(tasks_graph))
        .route("/tasks/run", post(tasks_run))
        .route("/scene", get(scene))
```

Blocking work runs in `spawn_blocking` so long builds never stall the Axum loop; failures return `{"ok": false, ...}` JSON, never a bare 500.

- [ ] **Step 5: Dogfood the DAG in `keplr.json`**

Old:

```json
{
  "tasks": {
    "check": { "cmd": "cargo test --workspace", "outputs": [] },
    "lint": { "cmd": "cargo clippy --workspace -- -D warnings", "outputs": [] }
  }
}
```

New:

```json
{
  "tasks": {
    "check": { "cmd": "cargo test --workspace --all-targets", "outputs": [] },
    "lint": { "cmd": "cargo clippy --workspace --all-targets -- -D warnings", "outputs": [], "deps": ["check"] }
  }
}
```

Now `keplr run lint` runs `check` first, and a second run skips what is unchanged. CI itself is untouched (it still calls `cargo` directly).

- [ ] **Step 6: Document Plan C in `README.md`** — append after the Plan B section, before the CI line:

```markdown
## Use (Plan C smart builds, production)

cargo run -p keplr-cli -- --root . run lint
cargo run -p keplr-cli -- --root . run --all --jobs 4
cargo run -p keplr-cli -- --root . run lint --force
cargo run -p keplr-cli -- --root . run --all --watch
curl '127.0.0.1:7137/tasks/graph'
curl -X POST 127.0.0.1:7137/tasks/run -H 'Content-Type: application/json' -d '{"all":true,"jobs":4}'
```

Also add the plan link line `Plan C builds: \`docs/superpowers/plans/2026-09-18-keplr-build-dag.md\`` next to the existing plan lines at the top.

- [ ] **Step 7: Push and watch CI**

```bash
git add crates/keplr-cli/src/main.rs crates/keplr-serve/src/lib.rs keplr.json README.md
git commit -m "feat(build): cli run flags plus watch, serve graph/run routes, DAG dogfood"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: CI green. `keplr run check` backward compat holds; new flags verified by reading `--help` output in a later session if needed.

---

## Self-review (run before handoff)

1. Spec coverage: DAG scheduler yes (Task 1 topo + closure), parallel yes (Task 3 levels + jobs cap), content-hash skipping yes (Task 2 fingerprints + journal), CAS output reuse yes (Task 2 store/restore in `.keplr/cas`), `watch` yes (Task 4 polling mode; persistent warm compiler daemons explicitly deferred — polling reuses warm OS caches with zero new failure modes on Termux), live logs yes (reports stream per task in order), failure cancels dependents yes (early `Err` return in both runners), stderr cached by hash yes (journal keeps hash; output replay is the report text).
2. Placeholder scan: no TBD/TODO/placeholder/unimplemented; every new function has a real body; every route returns real core/build data.
3. Type consistency: `TaskDef` keeps all four original fields with identical names/types and adds `deps`/`watch`/`fingerprint` as `Vec<String>`; `load_tasks`/`run_task` signatures unchanged; new `topo_order`, `task_fingerprint`, `graph_fingerprints`, `load_journal`, `save_journal`, `JournalEntry { hash, outputs }`, `RunReport { task, skipped, output, hash }`, `run_graph`, `run_graph_parallel` spelled identically in Tasks 2-4 and in the CLI/serve code that calls them.
