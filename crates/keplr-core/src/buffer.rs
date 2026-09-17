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
