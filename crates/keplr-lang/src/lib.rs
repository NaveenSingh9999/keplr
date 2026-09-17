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
