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
