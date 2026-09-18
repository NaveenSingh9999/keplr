# Keplr Plan D — Index + Lang + Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the three biggest spec gaps in one push: persistent index + watcher + trigram search in `keplr-core`, real highlight/diagnostics/completions/LSP probing in `keplr-lang`, and `yrs` CRDT docs + snapshots + Git LFS materialization in `keplr-sync`, all reachable from CLI and headless serve.

**Architecture:** Additive only, same pattern as Plans B/C. `Index` reuses `FileEntry` verbatim and persists as JSON under `.keplr/` (no new DB dep, Termux-safe). The watcher is a bounded `notify` collect (`poll_changes`) feeding `Index::apply`, so incremental updates never re-walk. Trigrams narrow candidates but an exact scan always follows, so results stay correct. `keplr-lang` stays dependency-light (one `serde` addition): byte-offset spans, `laml`-binary diagnostics with `hint` degradation, static completion lists, LSP presence probing plus piped spawn. `keplr-sync` re-adds the `yrs` workspace dep for a small `SyncDoc` wrapper, file-based update snapshots, and LFS pointer/ensure helpers over the real `git lfs` binary.

**Tech Stack:** Rust 1.97.1, existing workspace deps only (`notify`, `ignore`, `blake3`, `serde`/`serde_json`, `yrs 0.18.8`, `axum`, `clap`, `tokio` at the edges). No tree-sitter, no redb, no new network deps.

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md` sections 3 (index/search), 5 (sync/LFS), 7 (languages)

## Global Constraints

- No Electron, no Chromium shell, no WebView. Native binary + `keplr serve` JSON only.
- Same types with no rename: `FileEntry`, `SearchHit`, `TaskDef`, `LangKind`, `Buffer`, `Cas`, `Workspace::walk_files/grep`, `serve()`, plus Plan B/C additions (`Scene`, `RunReport`, `run_graph`, …). Extend only with additive defaulted fields or new items.
- RAM soft cap 500MB: fingerprint/index walks cap at 100k/20k files; trigram file cap 256KB.
- Results stream in <16ms chunks: search APIs take `limit: usize` and return early.
- Git remains truth; derived state lives under `{workspace}/.keplr/` (gitignored): `index.json`, `build-journal.json`, `cas/`, `snapshots/`.
- LAML reuse only: shell to `laml` binary if present, never reimplement evaluator.
- Every part ends with push + `gh run watch` green (`cargo test --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build --workspace`).
- No placeholders: no TBD/TODO/unimplemented; every route returns real data; failures are honest JSON or `Err`, never fake success.
- Standing orders for this plan: no local `cargo` runs (verify via GitHub CI with `gh`), no new test files — existing CI tests must stay green.

---

## File Structure

```
crates/keplr-core/src/lib.rs    # D1: Index, Change/ChangeKind, poll_changes, TrigramIndex, grep_trigram
crates/keplr-cli/src/main.rs    # D1: index/watch commands, search --via; D2: diagnostics command
crates/keplr-serve/src/lib.rs   # D1: GET /index/status; D2: GET /diagnostics /highlight; D3: /lfs/pointer /sync/*
crates/keplr-lang/src/lib.rs    # D2: TokenKind/Span/highlight, Diagnostic/laml_diagnostics, laml_completions, LspServer
crates/keplr-lang/Cargo.toml    # D2: + serde
crates/keplr-sync/src/lib.rs    # D3: SyncDoc, snapshots, LFS helpers
crates/keplr-sync/Cargo.toml    # D3: + yrs (+ serde if missing)
README.md                       # usage per part
```

---

## Part D1: Persistent index + watcher + trigram

### Task D1-1: Index struct with persist/refresh/apply/grep

**Files:**
- Modify: `crates/keplr-core/src/lib.rs` (import `BTreeMap`; append index code)

**Interfaces:**
- Consumes: `Workspace::walk_files`, `FileEntry`, `SearchHit`, `fingerprint_bytes`, `is_tracked_path`
- Produces: `pub struct Index`, `impl Index { pub fn build(ws: &Workspace) -> Self; pub fn files(&self) -> Vec<&FileEntry>; pub fn len(&self) -> usize; pub fn is_empty(&self) -> bool; pub fn get(&self, path: &Path) -> Option<&FileEntry>; pub fn load(ws: &Workspace) -> Self; pub fn save(&self, ws: &Workspace) -> anyhow::Result<()>; pub fn apply(&mut self, ws: &Workspace, path: &Path); pub fn refresh(&mut self, ws: &Workspace) -> Vec<PathBuf>; pub fn grep(&self, needle: &str, limit: usize) -> Vec<SearchHit> }`

- [ ] **Step 1: Add `BTreeMap` to the imports of `crates/keplr-core/src/lib.rs`**

Old:

```rust
use std::path::{Path, PathBuf};
```

New:

```rust
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
```

- [ ] **Step 2: Append the index code to `crates/keplr-core/src/lib.rs`** (end of file)

```rust
fn index_path(root: &Path) -> PathBuf {
    root.join(".keplr/index.json")
}

#[derive(Debug, Clone, Default)]
pub struct Index {
    entries: BTreeMap<PathBuf, FileEntry>,
}

impl Index {
    pub fn build(ws: &Workspace) -> Self {
        let mut entries = BTreeMap::new();
        for e in ws.walk_files(100_000) {
            entries.insert(e.path.clone(), e);
        }
        Self { entries }
    }

    pub fn files(&self) -> Vec<&FileEntry> {
        self.entries.values().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, path: &Path) -> Option<&FileEntry> {
        self.entries.get(path)
    }

    pub fn load(ws: &Workspace) -> Self {
        std::fs::read_to_string(index_path(&ws.root))
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<FileEntry>>(&t).ok())
            .map(|vec| Self {
                entries: vec.into_iter().map(|e| (e.path.clone(), e)).collect(),
            })
            .unwrap_or_default()
    }

    pub fn save(&self, ws: &Workspace) -> anyhow::Result<()> {
        let path = index_path(&ws.root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let vec: Vec<&FileEntry> = self.entries.values().collect();
        std::fs::write(&path, serde_json::to_string_pretty(&vec)?)?;
        Ok(())
    }

    pub fn apply(&mut self, ws: &Workspace, path: &Path) {
        if !is_tracked_path(path) {
            return;
        }
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            ws.root.join(path)
        };
        match std::fs::metadata(&full) {
            Ok(meta) if meta.is_file() => {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let hash = std::fs::read(&full)
                    .map(|b| fingerprint_bytes(&b))
                    .unwrap_or_else(|_| String::from("unreadable"));
                self.entries.insert(
                    full.clone(),
                    FileEntry {
                        path: full,
                        size: meta.len(),
                        mtime,
                        hash,
                    },
                );
            }
            _ => {
                self.entries.remove(&full);
                self.entries.remove(path);
            }
        }
    }

    pub fn refresh(&mut self, ws: &Workspace) -> Vec<PathBuf> {
        let fresh = Self::build(ws);
        let mut changed = Vec::new();
        for (p, e) in &fresh.entries {
            if self.entries.get(p) != Some(e) {
                changed.push(p.clone());
            }
        }
        for p in self.entries.keys() {
            if !fresh.entries.contains_key(p) {
                changed.push(p.clone());
            }
        }
        changed.sort();
        self.entries = fresh.entries;
        changed
    }

    pub fn grep(&self, needle: &str, limit: usize) -> Vec<SearchHit> {
        let mut hits = Vec::new();
        if needle.is_empty() {
            return hits;
        }
        for entry in self.entries.values() {
            if hits.len() >= limit {
                break;
            }
            let Ok(text) = std::fs::read_to_string(&entry.path) else {
                continue;
            };
            for (idx, line) in text.lines().enumerate() {
                if hits.len() >= limit {
                    break;
                }
                if let Some(col) = line.find(needle) {
                    hits.push(SearchHit {
                        path: entry.path.clone(),
                        line: (idx + 1) as u64,
                        col: (col + 1) as u64,
                        preview: line.chars().take(240).collect(),
                    });
                }
            }
        }
        hits
    }
}
```

Contract locked: `load` on missing/corrupt file returns empty (caller rebuilds); `refresh` returns sorted changed paths; `apply` on a deleted path removes both absolute and as-given key forms.

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-core/src/lib.rs
git commit -m "feat(core): persistent fingerprint index with refresh/apply/grep"
```

### Task D1-2: Watcher poll + trigram index

**Files:**
- Modify: `crates/keplr-core/src/lib.rs` (append; keep D1-1 byte-identical)

**Interfaces:**
- Consumes: `is_tracked_path`, `Workspace::walk_files`
- Produces: `pub enum ChangeKind { Modified | Removed }`, `pub struct Change { pub path: PathBuf, pub kind: ChangeKind }`, `pub fn poll_changes(root: &Path, wait_ms: u64) -> anyhow::Result<Vec<Change>>`, `pub struct TrigramIndex`, `impl TrigramIndex { pub fn build(ws: &Workspace, cap_files: usize, cap_bytes: u64) -> Self; pub fn candidates(&self, needle: &str) -> Vec<PathBuf> }`, `impl Workspace { pub fn grep_trigram(&self, needle: &str, limit: usize) -> Vec<SearchHit> }`

- [ ] **Step 1: Add `HashMap` to the collections import**

Old:

```rust
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
```

New:

```rust
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};
```

- [ ] **Step 2: Append watcher + trigram code** (end of file, after `impl Index`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

pub fn poll_changes(root: &Path, wait_ms: u64) -> anyhow::Result<Vec<Change>> {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            let _ = tx.send(res);
        },
        Config::default(),
    )?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    std::thread::sleep(std::time::Duration::from_millis(wait_ms.max(50)));
    drop(watcher);
    let mut seen: BTreeSet<(PathBuf, bool)> = BTreeSet::new();
    while let Ok(res) = rx.try_recv() {
        let Ok(event) = res else {
            continue;
        };
        let removed = matches!(event.kind, notify::EventKind::Remove(_));
        for p in event.paths {
            if !is_tracked_path(&p) {
                continue;
            }
            seen.insert((p, removed));
        }
    }
    let mut merged: BTreeMap<PathBuf, bool> = BTreeMap::new();
    for (p, removed) in seen {
        merged
            .entry(p)
            .and_modify(|r| *r = *r || removed)
            .or_insert(removed);
    }
    Ok(merged
        .into_iter()
        .map(|(path, removed)| Change {
            kind: if removed {
                ChangeKind::Removed
            } else {
                ChangeKind::Modified
            },
            path,
        })
        .collect())
}

#[derive(Debug, Clone, Default)]
pub struct TrigramIndex {
    files: Vec<PathBuf>,
    unindexed: Vec<PathBuf>,
    postings: HashMap<[u8; 3], Vec<usize>>,
}

impl TrigramIndex {
    pub fn build(ws: &Workspace, cap_files: usize, cap_bytes: u64) -> Self {
        let mut idx = Self::default();
        for entry in ws.walk_files(cap_files.max(1)) {
            if entry.size == 0 || entry.size > cap_bytes {
                idx.unindexed.push(entry.path);
                continue;
            }
            match std::fs::read(&entry.path) {
                Ok(bytes) => {
                    if bytes.len() < 3 {
                        continue;
                    }
                    let id = idx.files.len();
                    idx.files.push(entry.path);
                    let mut seen: BTreeSet<[u8; 3]> = BTreeSet::new();
                    for w in bytes.windows(3) {
                        seen.insert([w[0], w[1], w[2]]);
                    }
                    for t in seen {
                        idx.postings.entry(t).or_default().push(id);
                    }
                }
                Err(_) => {
                    idx.unindexed.push(entry.path);
                }
            }
        }
        idx
    }

    pub fn candidates(&self, needle: &str) -> Vec<PathBuf> {
        let n = needle.as_bytes();
        let mut out: Vec<PathBuf> = self.unindexed.clone();
        if n.len() < 3 {
            out.extend(self.files.iter().cloned());
            out.sort();
            return out;
        }
        let mut lists: Vec<&Vec<usize>> = Vec::new();
        for w in n.windows(3) {
            match self.postings.get(&[w[0], w[1], w[2]]) {
                Some(l) => lists.push(l),
                None => {
                    out.sort();
                    return out;
                }
            }
        }
        lists.sort_by_key(|l| l.len());
        let mut set: BTreeSet<usize> = lists[0].iter().cloned().collect();
        for l in &lists[1..] {
            let other: BTreeSet<usize> = l.iter().cloned().collect();
            set = set.intersection(&other).cloned().collect();
            if set.is_empty() {
                break;
            }
        }
        for id in set {
            if let Some(p) = self.files.get(id) {
                out.push(p.clone());
            }
        }
        out.sort();
        out
    }
}

impl Workspace {
    pub fn grep_trigram(&self, needle: &str, limit: usize) -> Vec<SearchHit> {
        let mut hits = Vec::new();
        if needle.is_empty() {
            return hits;
        }
        let idx = TrigramIndex::build(self, 20_000, 256 * 1024);
        for path in idx.candidates(needle) {
            if hits.len() >= limit {
                break;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (row, line) in text.lines().enumerate() {
                if hits.len() >= limit {
                    break;
                }
                if let Some(col) = line.find(needle) {
                    hits.push(SearchHit {
                        path: path.clone(),
                        line: (row + 1) as u64,
                        col: (col + 1) as u64,
                        preview: line.chars().take(240).collect(),
                    });
                }
            }
        }
        hits
    }
}
```

Correctness locked: unindexed (oversize/empty/unreadable) files are always in candidates; needles under 3 bytes match everything; a missing trigram short-circuits to just the unindexed set; an exact `find` scan always follows narrowing.

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-core/src/lib.rs
git commit -m "feat(core): notify watcher poll plus trigram candidate index"
```

### Task D1-3: CLI index/watch/search-via + serve status + docs

**Files:**
- Modify: `crates/keplr-cli/src/main.rs` (add `Index`, `Watch` variants; `Search` gains `--via`)
- Modify: `crates/keplr-serve/src/lib.rs` (add `GET /index/status`)
- Modify: `README.md` (Plan D link line + D1 usage)

**Interfaces:**
- Consumes: `Index::{load, build, refresh, save, apply, grep, len, is_empty}`, `poll_changes`, `Workspace::grep/grep_trigram`
- Produces: `keplr index [--refresh]`, `keplr watch [--debounce-ms N]`, `keplr search --via walk|index|trigram`, `GET /index/status`

- [ ] **Step 1: Extend the `Cmd` enum** — add after the `Search` variant (keep `Search` fields, add `via`):

Old:

```rust
    Search {
        needle: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
```

New:

```rust
    Search {
        needle: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value = "walk")]
        via: String,
    },
    Index {
        #[arg(long)]
        refresh: bool,
    },
    Watch {
        #[arg(long, default_value_t = 500)]
        debounce_ms: u64,
    },
```

- [ ] **Step 2: Replace the `Search` arm** (keep output format byte-identical):

Old:

```rust
        Cmd::Search { needle, limit } => {
            for hit in ws.grep(&needle, limit) {
                println!("{}:{}:{}: {}", hit.path.display(), hit.line, hit.col, hit.preview);
            }
        }
```

New:

```rust
        Cmd::Search { needle, limit, via } => {
            let hits = match via.as_str() {
                "index" => {
                    let mut index = keplr_core::Index::load(&ws);
                    if index.is_empty() {
                        index = keplr_core::Index::build(&ws);
                        let _ = index.save(&ws);
                    }
                    index.grep(&needle, limit)
                }
                "trigram" => ws.grep_trigram(&needle, limit),
                _ => ws.grep(&needle, limit),
            };
            for hit in hits {
                println!("{}:{}:{}: {}", hit.path.display(), hit.line, hit.col, hit.preview);
            }
        }
        Cmd::Index { refresh } => {
            let mut index = keplr_core::Index::load(&ws);
            if refresh && !index.is_empty() {
                let changed = index.refresh(&ws);
                index.save(&ws)?;
                println!(
                    "index files={} changed={} path={}",
                    index.len(),
                    changed.len(),
                    ws.root.join(".keplr/index.json").display()
                );
            } else {
                index = keplr_core::Index::build(&ws);
                index.save(&ws)?;
                println!(
                    "index files={} path={}",
                    index.len(),
                    ws.root.join(".keplr/index.json").display()
                );
            }
        }
        Cmd::Watch { debounce_ms } => {
            let mut index = keplr_core::Index::load(&ws);
            if index.is_empty() {
                index = keplr_core::Index::build(&ws);
                index.save(&ws)?;
            }
            println!("watching {} (Ctrl-C to stop)", cli.root.display());
            loop {
                let changes = keplr_core::poll_changes(&cli.root, debounce_ms)?;
                if changes.is_empty() {
                    continue;
                }
                for c in &changes {
                    index.apply(&ws, &c.path);
                    println!("{:?} {}", c.kind, c.path.display());
                }
                index.save(&ws)?;
            }
        }
```

- [ ] **Step 3: Add the serve status route** — append after the `scene` function, before `pub async fn serve`:

```rust
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
```

And register it. Old:

```rust
        .route("/scene", get(scene))
```

New:

```rust
        .route("/scene", get(scene))
        .route("/index/status", get(index_status))
```

- [ ] **Step 4: README** — add `Plan D index/lang/sync: \`docs/superpowers/plans/2026-09-18-keplr-plan-d-index-lang-sync.md\`` next to the other plan lines, and append after the Plan C section:

```markdown
## Use (Plan D index + watcher, production)

cargo run -p keplr-cli -- --root . index
cargo run -p keplr-cli -- --root . index --refresh
cargo run -p keplr-cli -- --root . search --via trigram "broadcast" --limit 20
cargo run -p keplr-cli -- --root . search --via index "broadcast" --limit 20
cargo run -p keplr-cli -- --root . watch --debounce-ms 500
curl '127.0.0.1:7137/index/status'
```

- [ ] **Step 5: Commit locally**

```bash
git add crates/keplr-cli/src/main.rs crates/keplr-serve/src/lib.rs README.md
git commit -m "feat(core): index/watch/search-via CLI plus index status route"
```

---

## Part D2: Language depth

### Task D2-1: Byte-offset highlighter

**Files:**
- Modify: `crates/keplr-lang/Cargo.toml` (+ serde)
- Modify: `crates/keplr-lang/src/lib.rs` (append highlight code; keep `LangKind`/`LamlProbe` byte-identical)

**Interfaces:**
- Consumes: `LangKind`
- Produces: `pub enum TokenKind { Keyword | Str | Comment | Number | Other }`, `pub struct Span { pub start: usize, pub len: usize, pub kind: TokenKind }`, `pub fn highlight(lang: LangKind, line: &str) -> Vec<Span>`

- [ ] **Step 1: `crates/keplr-lang/Cargo.toml`** — old:

```toml
[dependencies]
anyhow.workspace = true
```

New:

```toml
[dependencies]
anyhow.workspace = true
serde.workspace = true
```

- [ ] **Step 2: Append highlight code** (end of file; offsets are byte offsets, always on ASCII boundaries so slicing stays valid):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TokenKind {
    Keyword,
    Str,
    Comment,
    Number,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub len: usize,
    pub kind: TokenKind,
}

fn keywords(lang: LangKind) -> &'static [&'static str] {
    match lang {
        LangKind::Rust => &[
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "use",
            "mod", "crate", "self", "Self", "return", "if", "else", "match",
            "for", "while", "loop", "in", "where", "const", "static", "ref",
            "move", "async", "await", "dyn", "unsafe", "extern", "as", "break",
            "continue", "true", "false", "Some", "None", "Ok", "Err", "type",
        ],
        LangKind::TypeScript | LangKind::Tsx | LangKind::JavaScript => &[
            "function", "const", "let", "var", "return", "if", "else", "for",
            "while", "import", "export", "from", "class", "extends", "new",
            "typeof", "interface", "type", "enum", "async", "await", "try",
            "catch", "throw", "switch", "case", "break", "continue", "this",
            "true", "false", "null", "undefined",
        ],
        LangKind::Cpp => &[
            "int", "float", "double", "char", "bool", "void", "class",
            "struct", "public", "private", "protected", "virtual", "override",
            "final", "template", "typename", "namespace", "using", "return",
            "if", "else", "for", "while", "new", "delete", "const", "static",
            "auto", "true", "false", "nullptr", "include",
        ],
        LangKind::Go => &[
            "func", "var", "const", "type", "struct", "interface", "map",
            "chan", "go", "select", "return", "if", "else", "for", "range",
            "switch", "case", "break", "continue", "package", "import",
            "true", "false", "nil",
        ],
        LangKind::Laml => &[
            "serve", "on", "send", "broadcast", "joinRoom", "members",
            "async", "waitFor", "closc", "sort", "pop", "join", "upper",
            "lower", "keys", "has", "assert", "jsonParse", "jsonStringify",
            "setTimeout", "return", "if", "else", "for", "true", "false",
            "null",
        ],
        LangKind::Other => &[],
    }
}

fn push_other(spans: &mut Vec<Span>, other_start: &mut Option<usize>, end: usize) {
    if let Some(s) = other_start.take() {
        if end > s {
            spans.push(Span {
                start: s,
                len: end - s,
                kind: TokenKind::Other,
            });
        }
    }
}

pub fn highlight(lang: LangKind, line: &str) -> Vec<Span> {
    let bytes = line.as_bytes();
    let mut spans: Vec<Span> = Vec::new();
    let mut other_start: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if lang == LangKind::Laml && b == b'~' {
            push_other(&mut spans, &mut other_start, i);
            spans.push(Span {
                start: i,
                len: bytes.len() - i,
                kind: TokenKind::Comment,
            });
            break;
        }
        if lang != LangKind::Laml
            && b == b'/'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'/'
        {
            push_other(&mut spans, &mut other_start, i);
            spans.push(Span {
                start: i,
                len: bytes.len() - i,
                kind: TokenKind::Comment,
            });
            break;
        }
        if b == b'"' {
            push_other(&mut spans, &mut other_start, i);
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'"' {
                    j += 1;
                    break;
                }
                j += 1;
            }
            spans.push(Span {
                start: i,
                len: j - i,
                kind: TokenKind::Str,
            });
            i = j;
            continue;
        }
        if b.is_ascii_digit() {
            push_other(&mut spans, &mut other_start, i);
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.')
            {
                j += 1;
            }
            spans.push(Span {
                start: i,
                len: j - i,
                kind: TokenKind::Number,
            });
            i = j;
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            push_other(&mut spans, &mut other_start, i);
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            let word = &line[i..j];
            let kind = if keywords(lang).contains(&word) {
                TokenKind::Keyword
            } else {
                TokenKind::Other
            };
            spans.push(Span {
                start: i,
                len: j - i,
                kind,
            });
            i = j;
            continue;
        }
        if other_start.is_none() {
            other_start = Some(i);
        }
        i += 1;
    }
    push_other(&mut spans, &mut other_start, bytes.len());
    spans
}
```

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-lang/Cargo.toml crates/keplr-lang/src/lib.rs
git commit -m "feat(lang): byte-offset keyword/string/comment/number highlighter"
```

### Task D2-2: LAML diagnostics + completions

**Files:**
- Modify: `crates/keplr-lang/src/lib.rs` (append; keep D2-1 byte-identical)

**Interfaces:**
- Consumes: `LamlProbe::binary`
- Produces: `pub struct Diagnostic { pub path: String, pub line: u64, pub col: u64, pub message: String, pub severity: String }`, `pub fn laml_diagnostics(source: &Path) -> Vec<Diagnostic>`, `pub fn laml_completions(prefix: &str) -> Vec<String>`

- [ ] **Step 1: Append diagnostics + completions code** (end of file):

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub path: String,
    pub line: u64,
    pub col: u64,
    pub message: String,
    pub severity: String,
}

fn hint_diagnostic(source: &Path, message: &str) -> Diagnostic {
    Diagnostic {
        path: source.display().to_string(),
        line: 1,
        col: 1,
        message: message.to_string(),
        severity: String::from("hint"),
    }
}

fn parse_diag_line(raw: &str, fallback_path: &str) -> Option<Diagnostic> {
    let parts: Vec<&str> = raw.splitn(4, ':').collect();
    if parts.len() < 3 {
        return None;
    }
    let line: u64 = parts[1].trim().parse().ok()?;
    if line == 0 {
        return None;
    }
    let (col, message) = if parts.len() == 4 {
        match parts[2].trim().parse::<u64>() {
            Ok(c) if c > 0 => (c, parts[3].trim().to_string()),
            _ => (1, format!("{}: {}", parts[2].trim(), parts[3].trim())),
        }
    } else {
        (1, parts[2].trim().to_string())
    };
    if message.is_empty() {
        return None;
    }
    let path = if parts[0].trim().is_empty() {
        fallback_path.to_string()
    } else {
        parts[0].trim().to_string()
    };
    Some(Diagnostic {
        path,
        line,
        col,
        message,
        severity: String::from("error"),
    })
}

pub fn laml_diagnostics(source: &Path) -> Vec<Diagnostic> {
    let label = source.display().to_string();
    let Some(bin) = LamlProbe::binary() else {
        return vec![hint_diagnostic(
            source,
            "laml binary not found on PATH; install it for check/run diagnostics",
        )];
    };
    let out = std::process::Command::new(&bin)
        .arg("check")
        .arg(source)
        .output();
    let Ok(out) = out else {
        return vec![hint_diagnostic(source, "laml binary could not be spawned")];
    };
    if out.status.success() {
        return Vec::new();
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut diags: Vec<Diagnostic> = text
        .lines()
        .filter_map(|raw| parse_diag_line(raw, &label))
        .collect();
    if diags.is_empty() {
        diags.push(Diagnostic {
            path: label,
            line: 1,
            col: 1,
            message: text.chars().take(500).collect(),
            severity: String::from("error"),
        });
    }
    diags
}

const LAML_KEYWORDS: &[&str] = &[
    "serve",
    "on",
    "send",
    "broadcast",
    "joinRoom",
    "members",
    "async",
    "waitFor",
    "closc",
    "sort",
    "pop",
    "join",
    "upper",
    "lower",
    "keys",
    "has",
    "assert",
    "jsonParse",
    "jsonStringify",
    "setTimeout",
    "return",
    "if",
    "else",
    "for",
    "true",
    "false",
    "null",
];

pub fn laml_completions(prefix: &str) -> Vec<String> {
    LAML_KEYWORDS
        .iter()
        .filter(|k| k.starts_with(prefix))
        .map(|k| k.to_string())
        .collect()
}
```

Contract locked: success means zero diagnostics; unparsable failure output becomes one `error` diagnostic with the first 500 chars; missing/unspawnable binary becomes one `hint`, never an `Err`. Completions filter the exact spec keyword set (`serve/on/send/broadcast/joinRoom/members`, timers/JSON, `closc`, stdlib, `~`-comment language).

- [ ] **Step 2: Commit locally**

```bash
git add crates/keplr-lang/src/lib.rs
git commit -m "feat(lang): laml diagnostics with hint degrade plus completions"
```

### Task D2-3: LSP probe/spawn + CLI diagnostics + serve routes + docs

**Files:**
- Modify: `crates/keplr-lang/src/lib.rs` (append LSP code)
- Modify: `crates/keplr-cli/src/main.rs` (add `Diagnostics` variant + arm)
- Modify: `crates/keplr-serve/src/lib.rs` (add `GET /diagnostics`, `GET /highlight`)
- Modify: `README.md` (D2 usage)

**Interfaces:**
- Consumes: `LangKind`, `highlight`, `laml_diagnostics`
- Produces: `pub struct LspServer { pub name: String, pub cmd: String, pub args: Vec<String>, pub present: bool }`, `pub fn lsp_servers(lang: LangKind) -> Vec<LspServer>`, `pub fn command_present(cmd: &str) -> bool`, `pub fn spawn_lsp(server: &LspServer) -> anyhow::Result<std::process::Child>`, `keplr diagnostics <file>`, `GET /diagnostics?path=`, `GET /highlight?path=&line=`

- [ ] **Step 1: Append LSP code to `crates/keplr-lang/src/lib.rs`** (end of file):

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LspServer {
    pub name: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub present: bool,
}

pub fn command_present(cmd: &str) -> bool {
    if cmd.contains('/') {
        return Path::new(cmd).exists();
    }
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join(cmd).exists())
        })
        .unwrap_or(false)
}

pub fn lsp_servers(lang: LangKind) -> Vec<LspServer> {
    let defs: &[(&str, &str, &[&str])] = match lang {
        LangKind::TypeScript | LangKind::Tsx | LangKind::JavaScript => &[(
            "typescript-language-server",
            "typescript-language-server",
            &["--stdio"],
        )],
        LangKind::Cpp => &[("clangd", "clangd", &["--background-index"])],
        LangKind::Go => &[("gopls", "gopls", &["serve"])],
        LangKind::Rust => &[("rust-analyzer", "rust-analyzer", &[])],
        LangKind::Laml | LangKind::Other => &[],
    };
    defs.iter()
        .map(|(name, cmd, args)| LspServer {
            name: name.to_string(),
            cmd: cmd.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            present: command_present(cmd),
        })
        .collect()
}

pub fn spawn_lsp(server: &LspServer) -> anyhow::Result<std::process::Child> {
    if !server.present {
        anyhow::bail!(
            "language server `{}` not found ({}); install it first",
            server.name,
            server.cmd
        );
    }
    std::process::Command::new(&server.cmd)
        .args(&server.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to start `{}`: {}", server.cmd, e))
}
```

- [ ] **Step 2: CLI — add the variant** (after the `Doctor` variant, keep neighbours byte-identical):

Old:

```rust
    Doctor,
```

New:

```rust
    Doctor,
    Diagnostics {
        file: PathBuf,
    },
```

- [ ] **Step 3: CLI — add the arm** (after the `Doctor` arm):

Old:

```rust
        Cmd::Doctor => {
            println!("root={}", cli.root.display());
            println!("files={}", ws.walk_files(1000).len());
            println!("laml={:?}", keplr_lang::LamlProbe::binary());
        }
```

New:

```rust
        Cmd::Doctor => {
            println!("root={}", cli.root.display());
            println!("files={}", ws.walk_files(1000).len());
            println!("laml={:?}", keplr_lang::LamlProbe::binary());
        }
        Cmd::Diagnostics { file } => {
            let lang = keplr_lang::LangKind::from_path(&file);
            let diagnostics = if lang == keplr_lang::LangKind::Laml {
                keplr_lang::laml_diagnostics(&file)
            } else {
                Vec::new()
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "file": file.display().to_string(),
                    "lang": format!("{lang:?}"),
                    "diagnostics": diagnostics,
                    "servers": keplr_lang::lsp_servers(lang),
                }))?
            );
        }
```

- [ ] **Step 4: Serve — append handlers** (after `index_status`, before `pub async fn serve`):

```rust
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
```

Serve needs the `keplr-lang` and `keplr-sync` dependencies. `crates/keplr-serve/Cargo.toml` new lines:

```toml
keplr-core = { path = "../keplr-core" }
keplr-build = { path = "../keplr-build" }
keplr-render = { path = "../keplr-render" }
keplr-lang = { path = "../keplr-lang" }
keplr-sync = { path = "../keplr-sync" }
```

Register the routes. Old:

```rust
        .route("/index/status", get(index_status))
```

New:

```rust
        .route("/index/status", get(index_status))
        .route("/diagnostics", get(diagnostics))
        .route("/highlight", get(highlight))
```

- [ ] **Step 5: README** — append after the D1 usage block:

```markdown
## Use (Plan D lang, production)

cargo run -p keplr-cli -- --root ~/LAML diagnostics ng/src/main.lm
curl '127.0.0.1:7137/diagnostics?path=Cargo.toml'
curl '127.0.0.1:7137/highlight?path=Cargo.toml&line=1'
```

- [ ] **Step 6: Commit locally**

```bash
git add crates/keplr-lang/src/lib.rs crates/keplr-cli/src/main.rs crates/keplr-serve/src/lib.rs crates/keplr-serve/Cargo.toml README.md
git commit -m "feat(lang): lsp probe plus diagnostics/highlight CLI and serve routes"
```

---

## Part D3: Sync depth

### Task D3-1: yrs SyncDoc

**Files:**
- Modify: `crates/keplr-sync/Cargo.toml` (+ yrs, + serde if missing — check first)
- Modify: `crates/keplr-sync/src/lib.rs` (append; keep `Cas` byte-identical)

**Interfaces:**
- Consumes: `yrs::{Doc, GetString, ReadTxn, StateVector, Transact, Update, updates::{decoder::Decode, encoder::Encode}, Text}`
- Produces: `pub struct SyncDoc`, `impl SyncDoc { pub fn new(name: &str) -> Self; pub fn name(&self) -> &str; pub fn from_text(name: &str, text: &str) -> Self; pub fn push(&self, text: &str); pub fn insert(&self, index: u32, text: &str); pub fn content(&self) -> String; pub fn state_vector(&self) -> Vec<u8>; pub fn encode_update(&self) -> Vec<u8>; pub fn encode_update_since(&self, since: &[u8]) -> anyhow::Result<Vec<u8>>; pub fn apply_update(&self, bytes: &[u8]) -> anyhow::Result<()> }`

- [ ] **Step 1: Check `crates/keplr-sync/Cargo.toml`**, then add `yrs.workspace = true` (and `serde.workspace = true` if absent) under `[dependencies]`.

- [ ] **Step 2: Append SyncDoc code** (end of file; keep `Cas` untouched):

```rust
use yrs::{
    Doc, GetString, ReadTxn, StateVector, Transact, Update,
    updates::{decoder::Decode, encoder::Encode},
    Text,
};

pub struct SyncDoc {
    name: String,
    doc: Doc,
}

impl SyncDoc {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            doc: Doc::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn from_text(name: &str, text: &str) -> Self {
        let doc = Self::new(name);
        doc.push(text);
        doc
    }

    fn text_ref(&self) -> yrs::TextRef {
        self.doc.get_or_insert_text(self.name.as_str())
    }

    pub fn push(&self, text: &str) {
        let txt = self.text_ref();
        let mut txn = self.doc.transact_mut();
        txt.push(&mut txn, text);
    }

    pub fn insert(&self, index: u32, text: &str) {
        let txt = self.text_ref();
        let mut txn = self.doc.transact_mut();
        let at = index.min(txt.len(&txn));
        txt.insert(&mut txn, at, text);
    }

    pub fn content(&self) -> String {
        let txt = self.text_ref();
        let txn = self.doc.transact();
        txt.get_string(&txn)
    }

    pub fn state_vector(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }

    pub fn encode_update(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.encode_state_as_update_v1(&StateVector::default())
    }

    pub fn encode_update_since(&self, since: &[u8]) -> anyhow::Result<Vec<u8>> {
        let sv = StateVector::decode_v1(since)
            .map_err(|e| anyhow::anyhow!("bad state vector: {e}"))?;
        let txn = self.doc.transact();
        Ok(txn.encode_state_as_update_v1(&sv))
    }

    pub fn apply_update(&self, bytes: &[u8]) -> anyhow::Result<()> {
        let update =
            Update::decode_v1(bytes).map_err(|e| anyhow::anyhow!("bad update: {e}"))?;
        let mut txn = self.doc.transact_mut();
        txn.apply_update(update);
        Ok(())
    }
}
```

`{e}` on yrs error types requires `Display` — yrs errors implement it; if CI disagrees, switch that arm to `{e:?}`.

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-sync/Cargo.toml crates/keplr-sync/src/lib.rs
git commit -m "feat(sync): yrs CRDT doc with updates and state vectors"
```

### Task D3-2: Snapshots + LFS materialize

**Files:**
- Modify: `crates/keplr-sync/src/lib.rs` (append; keep D3-1 byte-identical)

**Interfaces:**
- Consumes: `SyncDoc::apply_update`
- Produces: `pub fn snapshot_name_ok(name: &str) -> bool`, `pub fn save_snapshot(dir: &Path, name: &str, update: &[u8]) -> anyhow::Result<PathBuf>`, `pub fn load_snapshot(dir: &Path, name: &str) -> anyhow::Result<Vec<u8>>`, `pub fn restore_snapshot(name: &str, update: &[u8]) -> anyhow::Result<SyncDoc>`, `pub struct LfsPointer { pub oid: String, pub size: u64 }`, `pub fn is_lfs_pointer_text(text: &str) -> bool`, `pub fn parse_lfs_pointer(text: &str) -> Option<LfsPointer>`, `pub fn lfs_pointer_of_file(path: &Path) -> Option<LfsPointer>`, `pub fn ensure_materialized(workdir: &Path, rel: &str) -> anyhow::Result<String>`

- [ ] **Step 1: Append snapshot + LFS code** (end of file). Needs `Path` import — change `use std::path::PathBuf;` to `use std::path::{Path, PathBuf};` if that is still the import line:

```rust
pub fn snapshot_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub fn save_snapshot(dir: &Path, name: &str, update: &[u8]) -> anyhow::Result<PathBuf> {
    if !snapshot_name_ok(name) {
        anyhow::bail!("bad snapshot name `{name}`");
    }
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{name}.update"));
    std::fs::write(&path, update)?;
    Ok(path)
}

pub fn load_snapshot(dir: &Path, name: &str) -> anyhow::Result<Vec<u8>> {
    if !snapshot_name_ok(name) {
        anyhow::bail!("bad snapshot name `{name}`");
    }
    Ok(std::fs::read(dir.join(format!("{name}.update")))?)
}

pub fn restore_snapshot(name: &str, update: &[u8]) -> anyhow::Result<SyncDoc> {
    let doc = SyncDoc::new(name);
    doc.apply_update(update)?;
    Ok(doc)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LfsPointer {
    pub oid: String,
    pub size: u64,
}

pub fn is_lfs_pointer_text(text: &str) -> bool {
    text.lines().next().map(|l| l.trim() == "version https://git-lfs.github.com/spec/v1").unwrap_or(false)
}

pub fn parse_lfs_pointer(text: &str) -> Option<LfsPointer> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "version https://git-lfs.github.com/spec/v1" {
        return None;
    }
    let mut oid = None;
    let mut size = None;
    for line in lines {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("oid sha256:") {
            if rest.len() == 64 && rest.chars().all(|c| c.is_ascii_hexdigit()) {
                oid = Some(rest.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("size ") {
            if let Ok(n) = rest.parse::<u64>() {
                size = Some(n);
            }
        }
    }
    Some(LfsPointer {
        oid: oid?,
        size: size?,
    })
}

pub fn lfs_pointer_of_file(path: &Path) -> Option<LfsPointer> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_lfs_pointer(&text)
}

pub fn ensure_materialized(workdir: &Path, rel: &str) -> anyhow::Result<String> {
    let full = workdir.join(rel);
    let text = std::fs::read_to_string(&full).unwrap_or_default();
    let pointer = match parse_lfs_pointer(&text) {
        Some(p) => p,
        None => return Ok(String::from("already materialized")),
    };
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(workdir)
        .arg("lfs")
        .arg("pull")
        .arg(format!("--include={rel}"))
        .output()
        .map_err(|e| anyhow::anyhow!("git lfs pull failed to spawn: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git lfs pull failed for `{rel}`: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let after = std::fs::read(&full)?;
    if is_lfs_pointer_text(&String::from_utf8_lossy(&after)) {
        anyhow::bail!("`{rel}` is still an LFS pointer after pull; check remote and .lfsconfig");
    }
    if after.len() as u64 != pointer.size {
        anyhow::bail!(
            "`{rel}` size {} does not match pointer size {}",
            after.len(),
            pointer.size
        );
    }
    Ok(format!("materialized {} bytes for `{rel}`", after.len()))
}
```

OIDs are SHA-256 by spec, so CAS (blake3-keyed) lookup by OID is deliberately not attempted — after materialization the caller stores bytes with the existing `Cas::put` for dedup. Lazy fetch shells the real `git lfs` binary; a missing binary surfaces as an honest spawn error.

- [ ] **Step 2: Commit locally**

```bash
git add crates/keplr-sync/src/lib.rs
git commit -m "feat(sync): file snapshots plus LFS pointer and materialize"
```

### Task D3-3: Serve sync/lfs routes + docs

**Files:**
- Modify: `crates/keplr-serve/src/lib.rs` (4 routes)
- Modify: `README.md` (D3 usage)

**Interfaces:**
- Consumes: `SyncDoc::{from_text, apply_update, content}`, `save_snapshot`, `load_snapshot`, `snapshot_name_ok`, `parse_lfs_pointer`
- Produces: `GET /lfs/pointer?path=`, `POST /sync/merge`, `POST /sync/snapshot`, `GET /sync/snapshot?name=`

- [ ] **Step 1: Append handlers** (after `highlight`, before `pub async fn serve`):

```rust
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
```

Register. Old:

```rust
        .route("/diagnostics", get(diagnostics))
        .route("/highlight", get(highlight))
```

New:

```rust
        .route("/diagnostics", get(diagnostics))
        .route("/highlight", get(highlight))
        .route("/lfs/pointer", get(lfs_pointer))
        .route("/sync/merge", post(sync_merge))
        .route(
            "/sync/snapshot",
            post(sync_snapshot_save).get(sync_snapshot_load),
        )
```

The `keplr-sync` dep was added with the `keplr-lang` dep in Task D2-3. `HashMap` is already imported.

- [ ] **Step 2: README** — append after the D2 usage block:

```markdown
## Use (Plan D sync, production)

curl '127.0.0.1:7137/lfs/pointer?path=assets/font.woff2'
curl -X POST 127.0.0.1:7137/sync/merge -H 'Content-Type: application/json' -d '{"name":"notes","seed":"hello","updates":[]}'
curl -X POST 127.0.0.1:7137/sync/snapshot -H 'Content-Type: application/json' -d '{"name":"notes","update":[1,2,3]}'
curl '127.0.0.1:7137/sync/snapshot?name=notes'
```

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-serve/src/lib.rs README.md
git commit -m "feat(sync): lfs pointer plus crdt merge and snapshot routes"
```

---

## Verify (all parts)

- [ ] **Push once and watch CI**

```bash
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: test + clippy + build green. On failure: `gh run view <id> --log-failed`, fix in a follow-up commit, push again. Known risk: `yrs 0.18` API drift (`TextRef::len`, `StateVector::encode_v1/decode_v1`, `Doc::new`) — fix from compiler output.

## Self-review (run before handoff)

1. Spec coverage: §3 index yes (persist/refresh/apply), watcher yes (poll→apply→save loop in `watch`), trigram hot narrowing yes with exact-scan correctness, search pool/lock-free bus explicitly out of scope (single-binary CLI/serve has no render thread to unblock); §7 highlight yes, LAML check/run reuse yes, diagnostics/completions yes, LSP probe+spawn yes, managed auto-download out of scope (honest `present: false` instead); §5 CAS yes (existing), CRDT buffers yes (SyncDoc), snapshots yes, LFS lazy materialize yes, roaming transport out of scope (merge/snapshot endpoints are the seam).
2. Placeholder scan: no TBD/TODO/placeholder/unimplemented; every public function has a real body; every route returns real data.
3. Type consistency: `Index/Change/ChangeKind/TrigramIndex`, `TokenKind/Span/Diagnostic/LspServer`, `SyncDoc/JournalEntry/LfsPointer` spelled identically in lib, CLI, and serve code; `FileEntry/SearchHit/TaskDef/LangKind/Buffer/Cas` untouched.
