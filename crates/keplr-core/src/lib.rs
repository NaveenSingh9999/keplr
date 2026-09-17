use std::path::{Path, PathBuf};

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
