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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Hunk {
    /// 1-based start line in the working-tree file.
    pub start: u64,
    /// Number of working-tree lines covered (0 = pure deletion at `start`).
    pub len: u64,
    pub kind: String,
}

/// Zero-context hunks for gutter bars. Returns (hunks, untracked).
pub fn file_hunks(workdir: &Path, rel: &str) -> anyhow::Result<(Vec<Hunk>, bool)> {
    if rel.contains("..") {
        anyhow::bail!("invalid path");
    }
    let tracked = git(workdir, &["ls-files", "--error-unmatch", "--", rel]).is_ok();
    if !tracked {
        return Ok((Vec::new(), true));
    }
    let out = git(workdir, &["diff", "-U0", "--", rel])?;
    let mut hunks = Vec::new();
    for line in out.lines() {
        if !line.starts_with("@@") {
            continue;
        }
        // Header: @@ -old[,len] +new[,len] @@
        let mut parts = line.split_whitespace();
        parts.next();
        let (mut old_len, mut new_start, mut new_len) = (1u64, 1u64, 1u64);
        if let Some(o) = parts.next() {
            let o = o.strip_prefix('-').unwrap_or(o);
            let mut it = o.split(',');
            it.next();
            if let Some(l) = it.next() {
                old_len = l.parse().unwrap_or(1);
            }
        }
        if let Some(n) = parts.next() {
            let n = n.strip_prefix('+').unwrap_or(n);
            let mut it = n.split(',');
            if let Some(s) = it.next() {
                new_start = s.parse().unwrap_or(1);
            }
            if let Some(l) = it.next() {
                new_len = l.parse().unwrap_or(1);
            }
        }
        let kind = if new_len == 0 {
            "del"
        } else if old_len == 0 {
            "add"
        } else {
            "mod"
        };
        hunks.push(Hunk {
            start: new_start.max(1),
            len: new_len,
            kind: kind.to_string(),
        });
    }
    Ok((hunks, false))
}

pub fn commit(workdir: &Path, message: &str) -> anyhow::Result<String> {
    git(workdir, &["add", "-A"])?;
    git(workdir, &["commit", "-m", message])
}

pub fn push(workdir: &Path, remote: Option<&str>, set_upstream: bool) -> anyhow::Result<String> {
    let remote = remote.filter(|r| !r.trim().is_empty()).unwrap_or("origin");
    if set_upstream {
        let branch = current_branch(workdir)?;
        git(workdir, &["push", "--set-upstream", remote, &branch])
    } else {
        git(workdir, &["push", remote])
    }
}

pub fn current_branch(workdir: &Path) -> anyhow::Result<String> {
    Ok(git(workdir, &["branch", "--show-current"])?.trim().to_string())
}

/// (ahead, behind) vs the upstream. Errors when no upstream is set.
pub fn ahead_behind(workdir: &Path) -> anyhow::Result<(u64, u64)> {
    let out = git(workdir, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])?;
    let mut it = out.split_whitespace();
    let ahead = it.next().unwrap_or("0").parse().unwrap_or(0);
    let behind = it.next().unwrap_or("0").parse().unwrap_or(0);
    Ok((ahead, behind))
}

pub fn file_diff(workdir: &Path, rel: &str, staged: bool) -> anyhow::Result<String> {
    if rel.contains("..") {
        anyhow::bail!("invalid path");
    }
    if staged {
        git(workdir, &["diff", "--cached", "--", rel])
    } else {
        git(workdir, &["diff", "--", rel])
    }
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

pub fn lfs_pull(workdir: &Path, include: Option<&str>) -> anyhow::Result<String> {
    match include {
        Some(pat) => git(workdir, &["lfs", "pull", &format!("--include={pat}")]),
        None => git(workdir, &["lfs", "pull"]),
    }
}

pub fn lfs_fetch(workdir: &Path, include: Option<&str>) -> anyhow::Result<String> {
    match include {
        Some(pat) => git(workdir, &["lfs", "fetch", &format!("--include={pat}")]),
        None => git(workdir, &["lfs", "fetch", "--all"]),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LfsTracked {
    pub path: String,
    pub oid: Option<String>,
    pub size: Option<u64>,
}

pub fn lfs_files(workdir: &Path) -> anyhow::Result<Vec<LfsTracked>> {
    let out = git(workdir, &["lfs", "ls-files", "--long"])?;
    let mut list = Vec::new();
    for line in out.lines() {
        let mut parts = line.split_whitespace();
        let (oid, size, path) = match (parts.next(), parts.next(), parts.next()) {
            (Some(a), Some(b), Some(c)) => (a, b, c),
            _ => continue,
        };
        let oid = oid.strip_prefix("oid sha256:").unwrap_or(oid);
        let oid = if oid.len() == 64 && oid.chars().all(|c| c.is_ascii_hexdigit()) {
            Some(oid.to_string())
        } else {
            None
        };
        let size = size.parse::<u64>().ok();
        list.push(LfsTracked {
            path: path.to_string(),
            oid,
            size,
        });
    }
    Ok(list)
}

pub fn clone_partial(
    url: &str,
    dir: &Path,
    depth: Option<u32>,
) -> anyhow::Result<String> {
    if url.trim().is_empty() {
        anyhow::bail!("empty url");
    }
    let mut args = vec![
        "clone".to_string(),
        "--filter=blob:none".to_string(),
        "--no-checkout".to_string(),
    ];
    if let Some(d) = depth {
        args.push("--depth".to_string());
        args.push(d.to_string());
    }
    args.push(url.to_string());
    args.push(dir.display().to_string());
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = std::process::Command::new("git")
        .args(&arg_refs)
        .output()
        .map_err(|e| anyhow::anyhow!("git failed to spawn: {e}"))?;
    if !out.status.success() {
        anyhow::bail!("git clone failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
