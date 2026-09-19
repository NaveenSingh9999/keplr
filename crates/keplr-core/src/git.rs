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

pub fn stage(workdir: &Path, path: &str) -> anyhow::Result<String> {
    git(workdir, &["add", "--", path])
}

pub fn unstage(workdir: &Path, path: &str) -> anyhow::Result<String> {
    git(workdir, &["restore", "--staged", "--", path])
}

pub fn discard(workdir: &Path, path: &str) -> anyhow::Result<String> {
    let entries = status(workdir)?;
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .ok_or_else(|| anyhow::anyhow!("no changes for `{path}`"))?;
    if entry.unstaged == '?' || entry.staged == '?' {
        let full = workdir.join(path);
        if full.is_dir() {
            std::fs::remove_dir_all(&full)?;
        } else {
            std::fs::remove_file(&full)?;
        }
        return Ok(String::from("removed untracked"));
    }
    git(workdir, &["restore", "--", path])
}

pub fn switch_branch(workdir: &Path, branch: &str, create: bool) -> anyhow::Result<String> {
    if branch.trim().is_empty() || branch.contains(char::is_whitespace) || branch.contains("..") {
        anyhow::bail!("bad branch name `{branch}`");
    }
    if create {
        git(workdir, &["switch", "-c", branch])
    } else {
        git(workdir, &["switch", branch])
    }
}

pub fn stash_push(workdir: &Path, message: &str) -> anyhow::Result<String> {
    if message.trim().is_empty() {
        git(workdir, &["stash", "push"])
    } else {
        git(workdir, &["stash", "push", "-m", message])
    }
}

pub fn stash_pop(workdir: &Path) -> anyhow::Result<String> {
    git(workdir, &["stash", "pop"])
}
