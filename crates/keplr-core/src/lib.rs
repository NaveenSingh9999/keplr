use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
};

pub mod buffer;
pub mod search;

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
        out.sort_by_key(|e| e.path.clone());
        out
    }

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

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

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
