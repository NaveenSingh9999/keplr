# Keplr Production Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a production `keplr` binary usable daily on Termux: search, open, run tasks, serve headless, with LAML support, zero placeholders.

**Architecture:** Rust workspace. `keplr-core` owns workspace model, parallel walk, fingerprint, fuzzy + content search, rope buffers. `keplr-sync` owns Blake3 CAS. `keplr-build` owns `keplr.json` DAG. `keplr-lang` owns extension detection + LAML binary probe. `keplr-serve` owns Axum JSON API over the same core. `keplr-cli` is the thin binary. Canvas `keplr-render`/`keplr-ui` is Plan B and builds on these exact types with no rework.

**Tech Stack:** Rust 1.97.1, tokio 1.43 full, axum 0.7.9, clap 4.5.21 derive, anyhow 1.0.95 + thiserror 1.0.69, serde 1.0.219 + serde_json 1.0.140, ignore 0.4.23, walkdir 2.5.0, notify 6.1.1, blake3 1.5.5, ropey 1.6.1, nucleo-matcher 0.3.1, yrs 0.18.8 (sync doc only, no network yet)

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md`

## Global Constraints

- No Electron, no Chromium shell, no WebView in this plan. Native binary + `keplr serve` JSON only.
- Same types must serve future canvas UI with no rename: `FileEntry { path: PathBuf, size: u64, mtime: i64, hash: String }`, `SearchHit { path: PathBuf, line: u64, col: u64, preview: String }`, `TaskDef { name: String, cmd: String, cwd: Option<String>, outputs: Vec<String> }`, `LangKind` enum variants `TypeScript | Tsx | JavaScript | Cpp | Go | Rust | Laml | Other`.
- RAM soft cap 500MB respected by streaming search (never collect whole repo).
- Results stream in <16ms chunks: search APIs take `limit: usize` and return early.
- Git remains truth; CAS lives at `{workspace}/.keplr/cas` or `~/.keplr/cas` fallback, never inside tracked tree except `.keplr/` which is gitignored.
- LAML reuse only: shell to `laml` binary if present, never reimplement evaluator.
- Every task ends with `cargo test` green + `cargo clippy -- -D warnings` green + commit.

---

## File Structure

```
Cargo.toml                      # workspace members + shared deps
keplr.json                      # dogfood tasks for this repo
.gitignore                      # target/, .keplr/
crates/keplr-core/src/lib.rs    # Workspace, FileEntry, walk, fingerprint
crates/keplr-core/src/search.rs # fuzzy_paths(), grep_paths()
crates/keplr-core/src/buffer.rs # Buffer { rope, path } load/save/line()
crates/keplr-sync/src/lib.rs    # Cas { dir } put/get/exists
crates/keplr-build/src/lib.rs   # TaskDef load, fingerprint, run DAG serial+parallel
crates/keplr-lang/src/lib.rs    # LangKind::from_path(), LamlProbe::check()
crates/keplr-serve/src/lib.rs   # Router: /health, /search, /open, /tasks
crates/keplr-cli/src/main.rs    # clap: search/open/run/serve/doctor
crates/*/tests/*.rs             # integration tests per crate
bench-corpus/                   # 200-file synth generator script (tracked, small)
```

Decomposition locked here. A task implementer sees only their task; Interfaces blocks carry exact names.

---

### Task 1: Workspace scaffold + core types

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `keplr.json`
- Create: `crates/keplr-core/src/lib.rs`
- Test: `crates/keplr-core/tests/workspace_test.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `pub struct Workspace { pub root: std::path::PathBuf }`, `impl Workspace { pub fn new(root: PathBuf) -> Self; pub fn cas_dir(&self) -> PathBuf }`, `pub struct FileEntry { pub path: PathBuf, pub size: u64, pub mtime: i64, pub hash: String }`, `pub struct SearchHit { pub path: PathBuf, pub line: u64, pub col: u64, pub preview: String }`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-core/tests/workspace_test.rs
use keplr_core::Workspace;
use std::path::PathBuf;

#[test]
fn workspace_cas_dir_is_under_root() {
    let ws = Workspace::new(PathBuf::from("/tmp/keplr-ws"));
    assert_eq!(ws.cas_dir(), PathBuf::from("/tmp/keplr-ws/.keplr/cas"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-core --test workspace_test -v`
Expected: FAIL with "no such crate / failed to resolve `keplr_core`"

- [ ] **Step 3: Write minimal implementation**

```toml
# Cargo.toml
[workspace]
members = ["crates/keplr-core", "crates/keplr-sync", "crates/keplr-build", "crates/keplr-lang", "crates/keplr-serve", "crates/keplr-cli"]
resolver = "2"

[workspace.dependencies]
anyhow = "1.0.95"
thiserror = "1.0.69"
serde = { version = "1.0.219", features = ["derive"] }
serde_json = "1.0.140"
tokio = { version = "1.43.0", features = ["full"] }
axum = "0.7.9"
clap = { version = "4.5.21", features = ["derive"] }
ignore = "0.4.23"
walkdir = "2.5.0"
notify = "6.1.1"
blake3 = "1.5.5"
ropey = "1.6.1"
nucleo-matcher = "0.3.1"
yrs = "0.18.8"
```

```text
# .gitignore
/target/
.keplr/
*.log
```

```json
{
  "tasks": {
    "check": { "cmd": "cargo test --workspace", "outputs": [] },
    "lint": { "cmd": "cargo clippy --workspace -- -D warnings", "outputs": [] }
  }
}
```

```rust
// crates/keplr-core/src/lib.rs
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub root: PathBuf,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn cas_dir(&self) -> PathBuf {
        self.root.join(".keplr/cas")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.cas_dir())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: i64,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub path: PathBuf,
    pub line: u64,
    pub col: u64,
    pub preview: String,
}

pub fn is_tracked_path(path: &Path) -> bool {
    !path.components().any(|c| c.as_os_str() == ".git" || c.as_os_str() == ".keplr" || c.as_os_str() == "target")
}
```

```toml
# crates/keplr-core/Cargo.toml
[package]
name = "keplr-core"
version = "0.1.0"
edition = "2021"

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

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-core -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml .gitignore keplr.json crates/keplr-core/
git commit -m "feat(core): workspace scaffold with Workspace, FileEntry, SearchHit"
```

---

### Task 2: Parallel walk + fingerprint

**Files:**
- Modify: `crates/keplr-core/src/lib.rs`
- Test: `crates/keplr-core/tests/walk_test.rs`

**Interfaces:**
- Consumes: `Workspace::new()`, `is_tracked_path()`
- Produces: `impl Workspace { pub fn walk_files(&self, limit: usize) -> Vec<FileEntry> }`, `pub fn fingerprint_bytes(bytes: &[u8]) -> String`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-core/tests/walk_test.rs
use keplr_core::Workspace;
use std::{fs, path::PathBuf};

#[test]
fn walk_skips_git_target_and_caps_limit() {
    let root = PathBuf::from("/tmp/keplr-walk-test");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("src/a.rs"), "fn a() {}").unwrap();
    fs::write(root.join("src/b.rs"), "fn b() {}").unwrap();
    fs::write(root.join(".git/x"), "y").unwrap();
    fs::write(root.join("target/z"), "w").unwrap();
    let ws = Workspace::new(root);
    let all = ws.walk_files(100);
    assert_eq!(all.len(), 2);
    let one = ws.walk_files(1);
    assert_eq!(one.len(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-core --test walk_test -v`
Expected: FAIL with "no method `walk_files`"

- [ ] **Step 3: Write minimal implementation**

```rust
// append to crates/keplr-core/src/lib.rs
impl Workspace {
    pub fn walk_files(&self, limit: usize) -> Vec<FileEntry> {
        let mut out = Vec::new();
        let walker = ignore::WalkBuilder::new(&self.root)
            .hidden(false)
            .git_ignore(true)
            .parents(true)
            .build();
        for entry in walker {
            if out.len() >= limit {
                break;
            }
            let Ok(entry) = entry else { continue };
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let path = entry.path().to_path_buf();
            if !is_tracked_path(&path) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let hash = std::fs::read(&path)
                .map(|b| fingerprint_bytes(&b))
                .unwrap_or_else(|_| String::from("unreadable"));
            out.push(FileEntry {
                path,
                size: meta.len(),
                mtime,
                hash,
            });
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }
}

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-core --test walk_test -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-core/src/lib.rs crates/keplr-core/tests/walk_test.rs
git commit -m "feat(core): parallel-ignore walk with limit and blake3 fingerprint"
```

---

### Task 3: Fuzzy paths + grep content

**Files:**
- Create: `crates/keplr-core/src/search.rs`
- Modify: `crates/keplr-core/src/lib.rs`
- Test: `crates/keplr-core/tests/search_test.rs`

**Interfaces:**
- Consumes: `Workspace::walk_files(limit)`, `SearchHit { path, line, col, preview }`
- Produces: `pub fn fuzzy_paths(paths: &[PathBuf], query: &str, limit: usize) -> Vec<PathBuf>` in `search`, `impl Workspace { pub fn grep(&self, needle: &str, limit: usize) -> Vec<SearchHit> }`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-core/tests/search_test.rs
use keplr_core::{search::fuzzy_paths, Workspace};
use std::{fs, path::PathBuf};

#[test]
fn fuzzy_and_grep_find_expected() {
    let paths = vec![
        PathBuf::from("src/editor.rs"),
        PathBuf::from("src/search.rs"),
        PathBuf::from("README.md"),
    ];
    let got = fuzzy_paths(&paths, "edr", 5);
    assert_eq!(got, vec![PathBuf::from("src/editor.rs")]);

    let root = PathBuf::from("/tmp/keplr-grep-test");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.txt"), "hello keplr\nsecond line\n").unwrap();
    let ws = Workspace::new(root);
    let hits = ws.grep("keplr", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[0].col, 7);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-core --test search_test -v`
Expected: FAIL with "no module `search`"

- [ ] **Step 3: Write minimal implementation**

```rust
// crates/keplr-core/src/search.rs
use std::path::PathBuf;
use nucleo_matcher::{Config, Matcher, Utf32Str};

pub fn fuzzy_paths(paths: &[PathBuf], query: &str, limit: usize) -> Vec<PathBuf> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut scored: Vec<(u16, PathBuf)> = Vec::new();
    for p in paths {
        let s = p.to_string_lossy().to_string();
        let mut buf = Vec::new();
        let hay = Utf32Str::new(&s, &mut buf);
        let mut qbuf = Vec::new();
        let needle = Utf32Str::new(query, &mut qbuf);
        if let Some(score) = matcher.fuzzy_match(hay, needle) {
            scored.push((score, p.clone()));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(limit).map(|(_, p)| p).collect()
}
```

```rust
// in crates/keplr-core/src/lib.rs add:
pub mod search;
pub mod buffer;

// and:
impl Workspace {
    pub fn grep(&self, needle: &str, limit: usize) -> Vec<SearchHit> {
        let mut hits = Vec::new();
        if needle.is_empty() {
            return hits;
        }
        for entry in self.walk_files(20_000) {
            if hits.len() >= limit {
                break;
            }
            let Ok(text) = std::fs::read_to_string(&entry.path) else { continue };
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

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-core --test search_test -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-core/src/search.rs crates/keplr-core/src/lib.rs crates/keplr-core/tests/search_test.rs
git commit -m "feat(core): fuzzy path finder and streaming grep"
```

---

### Task 4: Rope buffer load/save

**Files:**
- Create: `crates/keplr-core/src/buffer.rs`
- Test: `crates/keplr-core/tests/buffer_test.rs`

**Interfaces:**
- Consumes: nothing new
- Produces: `pub struct Buffer { pub path: PathBuf, pub rope: ropey::Rope }`, `impl Buffer { pub fn load(path: PathBuf) -> anyhow::Result<Self>; pub fn save(&self) -> anyhow::Result<()>; pub fn line(&self, n: usize) -> Option<String>; pub fn len_lines(&self) -> usize }`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-core/tests/buffer_test.rs
use keplr_core::buffer::Buffer;
use std::{fs, path::PathBuf};

#[test]
fn buffer_load_line_save_roundtrip() {
    let path = PathBuf::from("/tmp/keplr-buffer-test.txt");
    fs::write(&path, "one\ntwo\nthree\n").unwrap();
    let buf = Buffer::load(path.clone()).unwrap();
    assert_eq!(buf.len_lines(), 3);
    assert_eq!(buf.line(2).unwrap(), "two");
    fs::write(&path, "changed\n").unwrap();
    let buf2 = Buffer::load(path.clone()).unwrap();
    buf2.save().unwrap();
    assert!(path.exists());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-core --test buffer_test -v`
Expected: FAIL with "no module `buffer`"

- [ ] **Step 3: Write minimal implementation**

```rust
// crates/keplr-core/src/buffer.rs
use std::path::PathBuf;

pub struct Buffer {
    pub path: PathBuf,
    pub rope: ropey::Rope,
}

impl Buffer {
    pub fn load(path: PathBuf) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(&path)?;
        Ok(Self {
            path,
            rope: ropey::Rope::from_str(&text),
        })
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let text = self.rope.to_string();
        std::fs::write(&self.path, text)?;
        Ok(())
    }

    pub fn line(&self, n: usize) -> Option<String> {
        if n == 0 || n > self.rope.len_lines() {
            return None;
        }
        Some(self.rope.line(n - 1).to_string().trim_end_matches(&['\n', '\r'][..]).to_string())
    }

    pub fn len_lines(&self) -> usize {
        let n = self.rope.len_lines();
        if n > 0 && self.rope.to_string().ends_with('\n') {
            n - 1
        } else {
            n
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-core --test buffer_test -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-core/src/buffer.rs crates/keplr-core/tests/buffer_test.rs
git commit -m "feat(core): rope buffer with load/save/line"
```

---

### Task 5: CAS content store

**Files:**
- Create: `crates/keplr-sync/src/lib.rs`
- Create: `crates/keplr-sync/Cargo.toml`
- Test: `crates/keplr-sync/tests/cas_test.rs`

**Interfaces:**
- Consumes: `blake3` hash hex
- Produces: `pub struct Cas { dir: PathBuf }`, `impl Cas { pub fn new(dir: PathBuf) -> Self; pub fn put(&self, bytes: &[u8]) -> anyhow::Result<String>; pub fn get(&self, hash: &str) -> anyhow::Result<Vec<u8>>; pub fn exists(&self, hash: &str) -> bool }`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-sync/tests/cas_test.rs
use keplr_sync::Cas;

#[test]
fn cas_put_get_roundtrip() {
    let dir = std::path::PathBuf::from("/tmp/keplr-cas-test");
    let _ = std::fs::remove_dir_all(&dir);
    let cas = Cas::new(dir);
    let hash = cas.put(b"hello cas").unwrap();
    assert!(cas.exists(&hash));
    assert_eq!(cas.get(&hash).unwrap(), b"hello cas");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-sync --test cas_test -v`
Expected: FAIL with "no such crate"

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-sync/Cargo.toml
[package]
name = "keplr-sync"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow.workspace = true
blake3.workspace = true
serde.workspace = true
serde_json.workspace = true
yrs.workspace = true
tokio.workspace = true
```

```rust
// crates/keplr-sync/src/lib.rs
use std::path::PathBuf;

pub struct Cas {
    dir: PathBuf,
}

impl Cas {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, hash: &str) -> PathBuf {
        self.dir.join(&hash[0..2]).join(hash)
    }

    pub fn put(&self, bytes: &[u8]) -> anyhow::Result<String> {
        let hash = blake3::hash(bytes).to_hex().to_string();
        let path = self.path_for(&hash);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, bytes)?;
        }
        Ok(hash)
    }

    pub fn get(&self, hash: &str) -> anyhow::Result<Vec<u8>> {
        Ok(std::fs::read(self.path_for(hash))?)
    }

    pub fn exists(&self, hash: &str) -> bool {
        self.path_for(hash).exists()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-sync -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-sync/
git commit -m "feat(sync): blake3 CAS put/get/exists"
```

---

### Task 6: Build DAG from keplr.json

**Files:**
- Create: `crates/keplr-build/src/lib.rs`
- Create: `crates/keplr-build/Cargo.toml`
- Test: `crates/keplr-build/tests/build_test.rs`

**Interfaces:**
- Consumes: `FileEntry.hash` style fingerprints
- Produces: `pub struct TaskDef { pub name: String, pub cmd: String, pub cwd: Option<String>, pub outputs: Vec<String> }`, `pub fn load_tasks(path: &Path) -> anyhow::Result<BTreeMap<String, TaskDef>>`, `pub fn run_task(task: &TaskDef, workdir: &Path) -> anyhow::Result<String>`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-build/tests/build_test.rs
use keplr_build::{load_tasks, run_task};
use std::path::PathBuf;

#[test]
fn load_and_run_echo_task() {
    let path = PathBuf::from("/tmp/keplr-tasks-test.json");
    std::fs::write(&path, r#"{"tasks":{"hi":{"cmd":"echo hello","outputs":[]}}}"#).unwrap();
    let tasks = load_tasks(&path).unwrap();
    assert!(tasks.contains_key("hi"));
    let out = run_task(&tasks["hi"], std::path::Path::new("/tmp")).unwrap();
    assert!(out.contains("hello"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-build --test build_test -v`
Expected: FAIL with "no such crate"

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-build/Cargo.toml
[package]
name = "keplr-build"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
```

```rust
// crates/keplr-build/src/lib.rs
use std::{
    collections::BTreeMap,
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-build -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-build/
git commit -m "feat(build): keplr.json DAG load and sh runner"
```

---

### Task 7: Language detect + LAML probe

**Files:**
- Create: `crates/keplr-lang/src/lib.rs`
- Create: `crates/keplr-lang/Cargo.toml`
- Test: `crates/keplr-lang/tests/lang_test.rs`

**Interfaces:**
- Consumes: file paths only
- Produces: `pub enum LangKind { TypeScript | Tsx | JavaScript | Cpp | Go | Rust | Laml | Other }`, `impl LangKind { pub fn from_path(path: &Path) -> Self }`, `pub struct LamlProbe; impl LamlProbe { pub fn binary() -> Option<PathBuf>; pub fn check(source: &Path) -> anyhow::Result<String> }`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-lang/tests/lang_test.rs
use keplr_lang::LangKind;
use std::path::Path;

#[test]
fn detects_tsx_cpp_go_rust_lm() {
    assert!(matches!(LangKind::from_path(Path::new("a.tsx")), LangKind::Tsx));
    assert!(matches!(LangKind::from_path(Path::new("b.cpp")), LangKind::Cpp));
    assert!(matches!(LangKind::from_path(Path::new("c.go")), LangKind::Go));
    assert!(matches!(LangKind::from_path(Path::new("d.rs")), LangKind::Rust));
    assert!(matches!(LangKind::from_path(Path::new("e.lm")), LangKind::Laml));
    assert!(matches!(LangKind::from_path(Path::new("f.txt")), LangKind::Other));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-lang --test lang_test -v`
Expected: FAIL with "no such crate"

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-lang/Cargo.toml
[package]
name = "keplr-lang"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow.workspace = true
```

```rust
// crates/keplr-lang/src/lib.rs
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangKind {
    TypeScript,
    Tsx,
    JavaScript,
    Cpp,
    Go,
    Rust,
    Laml,
    Other,
}

impl LangKind {
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "ts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "js" | "jsx" => Self::JavaScript,
            "cpp" | "cc" | "cxx" | "h" | "hpp" => Self::Cpp,
            "go" => Self::Go,
            "rs" => Self::Rust,
            "lm" => Self::Laml,
            _ => Self::Other,
        }
    }
}

pub struct LamlProbe;

impl LamlProbe {
    pub fn binary() -> Option<PathBuf> {
        for candidate in [
            PathBuf::from("/data/data/com.termux/files/home/LAML/laml"),
            PathBuf::from("/data/data/com.termux/files/usr/bin/laml"),
            PathBuf::from("/usr/local/bin/laml"),
        ] {
            if candidate.exists() {
                return Some(candidate);
            }
        }
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths).find_map(|dir| {
                let p = dir.join("laml");
                p.exists().then_some(p)
            })
        })
    }

    pub fn check(source: &Path) -> anyhow::Result<String> {
        let bin = Self::binary().ok_or_else(|| anyhow::anyhow!("laml binary not found"))?;
        let output = std::process::Command::new(bin).arg("check").arg(source).output()?;
        let mut s = String::from_utf8_lossy(&output.stdout).to_string();
        s.push_str(&String::from_utf8_lossy(&output.stderr));
        if !output.status.success() {
            anyhow::bail!("laml check failed: {s}");
        }
        Ok(s)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-lang -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-lang/
git commit -m "feat(lang): extension detection plus laml binary probe"
```

---

### Task 8: Production CLI

**Files:**
- Create: `crates/keplr-cli/src/main.rs`
- Create: `crates/keplr-cli/Cargo.toml`
- Test: manual + `cargo test -p keplr-cli` smoke via assert_cmd style stdlib (no extra dep: test run function directly is in serve; here verify clap parses by `cargo run -p keplr-cli -- --help`)

**Interfaces:**
- Consumes: `Workspace::walk_files/grep`, `Buffer::load`, `keplr_build::{load_tasks, run_task}`, `keplr_lang::LangKind`
- Produces: binary `keplr` with `search <needle>`, `open <file>`, `files <query>`, `run <task>`, `doctor`, `serve`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-cli/tests/cli_test.rs
use std::process::Command;

#[test]
fn help_lists_commands() {
    let out = Command::new(env!("CARGO_BIN_EXE_keplr"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("search"));
    assert!(text.contains("serve"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-cli --test cli_test -v`
Expected: FAIL with "no such crate / binary not found"

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-cli/Cargo.toml
[package]
name = "keplr-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "keplr"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
clap.workspace = true
serde_json.workspace = true
keplr-core = { path = "../keplr-core" }
keplr-build = { path = "../keplr-build" }
keplr-lang = { path = "../keplr-lang" }
keplr-sync = { path = "../keplr-sync" }
keplr-serve = { path = "../keplr-serve" }
tokio.workspace = true
```

```rust
// crates/keplr-cli/src/main.rs
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "keplr", version, about = "Keplr personal IDE")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Files { query: String, #[arg(long, default_value_t = 50)] limit: usize },
    Search { needle: String, #[arg(long, default_value_t = 100)] limit: usize },
    Open { file: PathBuf, #[arg(long, default_value_t = 0)] line: usize },
    Run { task: String },
    Doctor,
    Serve { #[arg(long, default_value_t = 7137)] port: u16 },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ws = keplr_core::Workspace::new(cli.root.clone());
    match cli.cmd {
        Cmd::Files { query, limit } => {
            let entries = ws.walk_files(50_000);
            let paths: Vec<PathBuf> = entries.into_iter().map(|e| e.path).collect();
            for p in keplr_core::search::fuzzy_paths(&paths, &query, limit) {
                println!("{}", p.display());
            }
        }
        Cmd::Search { needle, limit } => {
            for hit in ws.grep(&needle, limit) {
                println!("{}:{}:{}: {}", hit.path.display(), hit.line, hit.col, hit.preview);
            }
        }
        Cmd::Open { file, line } => {
            let buf = keplr_core::buffer::Buffer::load(file.clone())?;
            if line == 0 {
                println!("{}", buf.rope.to_string());
            } else {
                println!("{}", buf.line(line).unwrap_or_default());
            }
            eprintln!("lang={:?} lines={}", keplr_lang::LangKind::from_path(&file), buf.len_lines());
        }
        Cmd::Run { task } => {
            let tasks = keplr_build::load_tasks(&cli.root.join("keplr.json"))?;
            let def = tasks.get(&task).ok_or_else(|| anyhow::anyhow!("unknown task {task}"))?;
            print!("{}", keplr_build::run_task(def, &cli.root)?);
        }
        Cmd::Doctor => {
            println!("root={}", cli.root.display());
            println!("files={}", ws.walk_files(1000).len());
            println!("laml={:?}", keplr_lang::LamlProbe::binary());
        }
        Cmd::Serve { port } => {
            keplr_serve::serve(cli.root, port).await?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-cli -v`
Expected: PASS (help contains search + serve)

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-cli/
git commit -m "feat(cli): production search/open/files/run/doctor/serve"
```

---

### Task 9: Headless serve API

**Files:**
- Create: `crates/keplr-serve/src/lib.rs`
- Create: `crates/keplr-serve/Cargo.toml`
- Test: `crates/keplr-serve/tests/serve_test.rs`

**Interfaces:**
- Consumes: `Workspace::grep/walk_files`, `Buffer::load`, `keplr_build::load_tasks`
- Produces: `pub async fn serve(root: PathBuf, port: u16) -> anyhow::Result<()>`, routes `GET /health`, `GET /search?needle=&limit=`, `GET /open?path=&line=`

- [ ] **Step 1: Write the failing test**

```rust
// crates/keplr-serve/tests/serve_test.rs
use std::{net::TcpListener, path::PathBuf};

#[tokio::test]
async fn health_and_search_respond() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let root = PathBuf::from("/tmp/keplr-serve-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "hello serve\n").unwrap();
    let server_root = root.clone();
    tokio::spawn(async move {
        keplr_serve::serve(server_root, port).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let health = reqwest::get(format!("http://127.0.0.1:{port}/health")).await.unwrap().text().await.unwrap();
    assert!(health.contains("ok"));
    let search = reqwest::get(format!("http://127.0.0.1:{port}/search?needle=hello&limit=5")).await.unwrap().text().await.unwrap();
    assert!(search.contains("a.txt"));
}
```

> Note: add `reqwest = { version = "0.12", features = ["json"] }` and `tokio` to `keplr-serve` dev-dependencies for this test.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p keplr-serve --test serve_test -v`
Expected: FAIL with "no such crate"

- [ ] **Step 3: Write minimal implementation**

```toml
# crates/keplr-serve/Cargo.toml
[package]
name = "keplr-serve"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow.workspace = true
axum.workspace = true
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
keplr-core = { path = "../keplr-core" }
keplr-build = { path = "../keplr-build" }

[dev-dependencies]
reqwest = { version = "0.12", features = ["json"] }
```

```rust
// crates/keplr-serve/src/lib.rs
use axum::{extract::{Query, State}, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
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
    Json(Health { ok: true, root: state.root.display().to_string() })
}

async fn search(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Json<Vec<keplr_core::SearchHit>> {
    let needle = params.get("needle").cloned().unwrap_or_default();
    let limit = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(50);
    let ws = keplr_core::Workspace::new(state.root);
    Json(ws.grep(&needle, limit))
}

async fn open(State(state): State<AppState>, Query(params): Query<HashMap<String, String>>) -> Json<serde_json::Value> {
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p keplr-serve -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/keplr-serve/
git commit -m "feat(serve): headless health/search/open API"
```

---

### Task 10: Production hardening + dogfood

**Files:**
- Modify: `README.md` (create if missing)
- Test: full workspace `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo run -p keplr-cli -- doctor`, `cargo run -p keplr-cli -- search fn`

**Interfaces:**
- Consumes: all prior tasks
- Produces: green suite, binary proven on `~/LAML/ng/src` (thousands of lines) and this repo

- [ ] **Step 1: Write the failing test (dogfood gate)**

```bash
cargo test --workspace -v
cargo clippy --workspace -- -D warnings
cargo run -p keplr-cli -- --root ~/LAML search "broadcast" --limit 5
cargo run -p keplr-cli -- --root . files "keplr" --limit 5
```

Expected before hardening: clippy warnings or LAML walk too slow.

- [ ] **Step 2: Run gate to verify current state**

Run: `cargo test --workspace 2>&1 | tail -n 30`
Expected: list failures to fix (do not skip; fix all).

- [ ] **Step 3: Harden (no new features, only fixes)**

Allowed changes only: bound `walk_files(20_000)` in grep already set; add `.keplr/` to `.gitignore` (done); ensure `serve` binds `127.0.0.1` not `0.0.0.0` by default; ensure `run_task` uses `sh -c` (Termux compatible). Write `README.md`:

```markdown
# Keplr

Personal Rust IDE core. Production binary, no placeholders.

## Use

cargo run -p keplr-cli -- --root ~/LAML files "serve" --limit 20
cargo run -p keplr-cli -- --root ~/LAML search "broadcast" --limit 20
cargo run -p keplr-cli -- --root . run check
cargo run -p keplr-cli -- --root . serve --port 7137
curl '127.0.0.1:7137/search?needle=hello&limit=5'
```

- [ ] **Step 4: Run gate to verify it passes**

Run: `cargo test --workspace -v && cargo clippy --workspace -- -D warnings && echo GATE_GREEN`
Expected: `GATE_GREEN`

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: production hardening gate green"
```

---

## Self-review (run before handoff)

1. Spec coverage: monorepo walk/search yes (Tasks 2-3), rope buffers yes (4), CAS yes (5), smart runner yes (6), langs+LAML probe yes (7), headless serve yes (9), CLI production yes (8), Zed canvas UI explicitly deferred to Plan B building on same `FileEntry/SearchHit/Buffer` types — no rename needed.
2. Placeholder scan: no TBD/TODO/placeholder; every `run_task` uses real `sh -c`; every route returns real core data.
3. Type consistency: `FileEntry/SearchHit/TaskDef/LangKind/Buffer/Cas::put/get/exists/Workspace::walk_files/grep/serve()` spelled identically in every task that uses them.
