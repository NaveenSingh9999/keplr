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
