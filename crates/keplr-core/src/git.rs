use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StatusEntry {
    pub path: String,
    pub staged: char,
    pub unstaged: char,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Branch {
    pub name: String,
    pub current: bool,
}

fn git(workdir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("git failed to spawn: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn xy(pair: &str) -> (char, char) {
    let mut c = pair.chars();
    (c.next().unwrap_or(' '), c.next().unwrap_or(' '))
}

pub fn status(workdir: &Path) -> anyhow::Result<Vec<StatusEntry>> {
    let out = git(workdir, &["status", "--porcelain=v1", "-z", "--untracked-files=normal"])?;
    let mut entries = Vec::new();
    for rec in out.split('\0') {
        if rec.len() < 4 {
            continue;
        }
        let (staged, unstaged) = xy(&rec[..2]);
        let mut path = rec[3..].to_string();
        if path.contains(" -> ") {
            path = path.split(" -> ").last().unwrap_or(&path).to_string();
        }
        entries.push(StatusEntry {
            path,
            staged,
            unstaged,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

pub fn log(workdir: &Path, limit: usize) -> anyhow::Result<Vec<LogEntry>> {
    let n = limit.clamp(1, 200).to_string();
    let out = git(
        workdir,
        &[
            "log",
            "--format=%H%x1f%an%x1f%ad%x1f%s",
            "--date=short",
            "-n",
            &n,
        ],
    )?;
    let mut entries = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split('\x1f').collect();
        if f.len() != 4 {
            continue;
        }
        entries.push(LogEntry {
            hash: f[0].to_string(),
            author: f[1].to_string(),
            date: f[2].to_string(),
            message: f[3].to_string(),
        });
    }
    Ok(entries)
}

pub fn branches(workdir: &Path) -> anyhow::Result<Vec<Branch>> {
    let out = git(workdir, &["branch", "--list"])?;
    let mut list = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let (current, name) = match t.strip_prefix("* ") {
            Some(n) => (true, n),
            None => (false, t),
        };
        let name = match name.strip_prefix("+ ") {
            Some(n) => n,
            None => name,
        };
        list.push(Branch {
            name: name.to_string(),
            current,
        });
    }
    Ok(list)
}

pub fn diff_stat(workdir: &Path) -> anyhow::Result<String> {
    git(workdir, &["diff", "--stat"])
}

pub fn commit(workdir: &Path, message: &str) -> anyhow::Result<String> {
    git(workdir, &["add", "-A"])?;
    git(workdir, &["commit", "-m", message])
}
