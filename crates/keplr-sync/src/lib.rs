use std::path::{Path, PathBuf};

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

use yrs::{Doc, GetString, ReadTxn, StateVector, Transact, Update};

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
