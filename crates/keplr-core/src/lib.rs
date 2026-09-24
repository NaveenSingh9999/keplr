use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};

pub mod buffer;
pub mod search;
#[cfg(not(target_arch = "wasm32"))]
pub mod git;

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

    #[cfg(not(target_arch = "wasm32"))]
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.cas_dir())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: i64,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub path: PathBuf,
    pub line: u64,
    pub col: u64,
    pub preview: String,
}

pub fn is_tracked_path(path: &Path) -> bool {
    !path.components().any(|c| c.as_os_str() == ".git" || c.as_os_str() == ".keplr" || c.as_os_str() == "target")
}

impl Workspace {
    #[cfg(not(target_arch = "wasm32"))]
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
            let full = entry.path().to_path_buf();
            if !is_tracked_path(&full) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&full) else { continue };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let hash = std::fs::read(&full)
                .map(|b| fingerprint_bytes(&b))
                .unwrap_or_else(|_| String::from("unreadable"));
            // Always store root-relative paths: the walker joins entries onto
            // root, so an absolute root (e.g. from `keplr start`) used to leak
            // absolute paths here, which the server's path gate then rejected.
            let path = full
                .strip_prefix(&self.root)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|_| full.clone());
            out.push(FileEntry {
                path,
                size: meta.len(),
                mtime,
                hash,
            });
        }
        out.sort_by_key(|e| e.path.clone());
        out
    }

    #[cfg(target_arch = "wasm32")]
    pub fn walk_files(&self, _limit: usize) -> Vec<FileEntry> {
        Vec::new()
    }

    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(target_arch = "wasm32")]
    pub fn grep(&self, _needle: &str, _limit: usize) -> Vec<SearchHit> {
        Vec::new()
    }
}

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn index_path(root: &Path) -> PathBuf {
    root.join(".keplr/index.json")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexState {
    Fresh,
    Missing,
    Corrupt,
}

#[derive(Debug, Clone, Default)]
pub struct Index {
    entries: BTreeMap<PathBuf, FileEntry>,
}

impl Index {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn build(ws: &Workspace) -> Self {
        let mut entries = BTreeMap::new();
        for e in ws.walk_files(100_000) {
            entries.insert(e.path.clone(), e);
        }
        Self { entries }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn build(_ws: &Workspace) -> Self {
        Self::default()
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

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(ws: &Workspace) -> Self {
        Self::load_status(ws).0
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load(_ws: &Workspace) -> Self {
        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_status(ws: &Workspace) -> (Self, IndexState) {
        match std::fs::read_to_string(index_path(&ws.root)) {
            Err(_) => (Self::default(), IndexState::Missing),
            Ok(t) => match serde_json::from_str::<Vec<FileEntry>>(&t) {
                Ok(vec) => (
                    Self {
                        // Heal indexes written before paths went root-relative:
                        // absolutize nothing, just demote in-root absolutes.
                        entries: vec
                            .into_iter()
                            .map(|mut e| {
                                if e.path.is_absolute() {
                                    if let Ok(rel) = e.path.strip_prefix(&ws.root).map(|p| p.to_path_buf()) {
                                        e.path = rel;
                                    }
                                }
                                (e.path.clone(), e)
                            })
                            .collect(),
                    },
                    IndexState::Fresh,
                ),
                Err(_) => (Self::default(), IndexState::Corrupt),
            },
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load_status(_ws: &Workspace) -> (Self, IndexState) {
        (Self::default(), IndexState::Missing)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self, ws: &Workspace) -> anyhow::Result<()> {
        let path = index_path(&ws.root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let vec: Vec<&FileEntry> = self.entries.values().collect();
        std::fs::write(&path, serde_json::to_string_pretty(&vec)?)?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save(&self, _ws: &Workspace) -> anyhow::Result<()> {
        anyhow::bail!("no filesystem on wasm")
    }

    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(target_arch = "wasm32")]
    pub fn apply(&mut self, _ws: &Workspace, _path: &Path) {}

    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(target_arch = "wasm32")]
    pub fn refresh(&mut self, _ws: &Workspace) -> Vec<PathBuf> {
        Vec::new()
    }

    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(target_arch = "wasm32")]
    pub fn grep(&self, _needle: &str, _limit: usize) -> Vec<SearchHit> {
        Vec::new()
    }
}

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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub fn poll_changes(_root: &Path, _wait_ms: u64) -> anyhow::Result<Vec<Change>> {
    anyhow::bail!("notify unavailable on wasm")
}

#[derive(Debug, Clone, Default)]
pub struct TrigramIndex {
    files: Vec<PathBuf>,
    unindexed: Vec<PathBuf>,
    postings: HashMap<[u8; 3], Vec<usize>>,
}

impl TrigramIndex {
    #[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
impl TrigramIndex {
    pub fn build(_ws: &Workspace, _cap_files: usize, _cap_bytes: u64) -> Self {
        Self::default()
    }
}

impl Workspace {
    #[cfg(not(target_arch = "wasm32"))]
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

    #[cfg(target_arch = "wasm32")]
    pub fn grep_trigram(&self, _needle: &str, _limit: usize) -> Vec<SearchHit> {
        Vec::new()
    }
}

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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

fn lcg_next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

#[cfg(not(target_arch = "wasm32"))]
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
