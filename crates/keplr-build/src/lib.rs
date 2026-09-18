use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDef {
    pub name: String,
    pub cmd: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub deps: Vec<String>,
    #[serde(default)]
    pub watch: Vec<String>,
    #[serde(default)]
    pub fingerprint: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FileShape {
    #[serde(default)]
    tasks: BTreeMap<String, RawTask>,
}

#[derive(Debug, Deserialize)]
struct RawTask {
    cmd: String,
    cwd: Option<String>,
    #[serde(default)]
    outputs: Vec<String>,
    #[serde(default)]
    deps: Vec<String>,
    #[serde(default)]
    watch: Vec<String>,
    #[serde(default)]
    fingerprint: Vec<String>,
}

pub fn load_tasks(path: &Path) -> anyhow::Result<BTreeMap<String, TaskDef>> {
    let text = std::fs::read_to_string(path)?;
    let shape: FileShape = serde_json::from_str(&text)?;
    Ok(shape
        .tasks
        .into_iter()
        .map(|(name, raw)| {
            (
                name.clone(),
                TaskDef {
                    name,
                    cmd: raw.cmd,
                    cwd: raw.cwd,
                    outputs: raw.outputs,
                    deps: raw.deps,
                    watch: raw.watch,
                    fingerprint: raw.fingerprint,
                },
            )
        })
        .collect())
}

pub fn run_task(task: &TaskDef, workdir: &Path) -> anyhow::Result<String> {
    let cwd = task.cwd.as_ref().map(Path::new).unwrap_or(workdir);
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&task.cmd)
        .current_dir(cwd)
        .output()?;
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        anyhow::bail!("task `{}` failed: {}", task.name, combined);
    }
    Ok(combined)
}

fn topo_levels(
    tasks: &BTreeMap<String, TaskDef>,
    wanted: &BTreeSet<String>,
) -> anyhow::Result<Vec<Vec<String>>> {
    let mut indeg: BTreeMap<String, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in wanted {
        let task = tasks
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
        let mut count = 0;
        for dep in &task.deps {
            if !tasks.contains_key(dep) {
                anyhow::bail!("task `{name}` depends on unknown task `{dep}`");
            }
            if wanted.contains(dep) {
                count += 1;
                dependents.entry(dep.clone()).or_default().push(name.clone());
            }
        }
        indeg.insert(name.clone(), count);
    }
    let mut ready: BTreeSet<String> = indeg
        .iter()
        .filter(|(_, &c)| c == 0)
        .map(|(n, _)| n.clone())
        .collect();
    let mut levels = Vec::new();
    while !ready.is_empty() {
        let level: Vec<String> = ready.iter().cloned().collect();
        ready.clear();
        for name in &level {
            if let Some(ds) = dependents.get(name) {
                for d in ds {
                    if let Some(c) = indeg.get_mut(d) {
                        *c -= 1;
                        if *c == 0 {
                            ready.insert(d.clone());
                        }
                    }
                }
            }
        }
        levels.push(level);
    }
    let done: usize = levels.iter().map(Vec::len).sum();
    if done != wanted.len() {
        let stuck: Vec<String> = indeg
            .into_iter()
            .filter(|(_, c)| *c > 0)
            .map(|(n, _)| n)
            .collect();
        anyhow::bail!("dependency cycle among: {}", stuck.join(", "));
    }
    Ok(levels)
}

pub fn topo_order(tasks: &BTreeMap<String, TaskDef>) -> anyhow::Result<Vec<String>> {
    let all: BTreeSet<String> = tasks.keys().cloned().collect();
    Ok(topo_levels(tasks, &all)?.into_iter().flatten().collect())
}

fn closure(tasks: &BTreeMap<String, TaskDef>, targets: &[String]) -> anyhow::Result<BTreeSet<String>> {
    let mut wanted = BTreeSet::new();
    let mut stack: Vec<String> = targets.to_vec();
    while let Some(name) = stack.pop() {
        if !wanted.insert(name.clone()) {
            continue;
        }
        let task = tasks
            .get(&name)
            .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
        for dep in &task.deps {
            stack.push(dep.clone());
        }
    }
    Ok(wanted)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JournalEntry {
    pub hash: String,
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunReport {
    pub task: String,
    pub skipped: bool,
    pub output: String,
    pub hash: String,
}

fn resolve_under(workdir: &Path, pat: &str) -> PathBuf {
    let p = Path::new(pat);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        workdir.join(p)
    }
}

fn hash_file(hasher: &mut blake3::Hasher, path: &Path) {
    match std::fs::read(path) {
        Ok(bytes) => {
            hasher.update(&bytes);
        }
        Err(_) => {
            hasher.update(b"\0missing:");
            hasher.update(path.to_string_lossy().as_bytes());
        }
    }
    hasher.update(&[0]);
}

fn hash_listed(hasher: &mut blake3::Hasher, workdir: &Path, patterns: &[String]) {
    let mut pats: Vec<&String> = patterns.iter().collect();
    pats.sort();
    for pat in pats {
        let p = resolve_under(workdir, pat);
        hasher.update(pat.as_bytes());
        hasher.update(&[0]);
        if p.is_file() {
            hash_file(hasher, &p);
        } else if p.is_dir() {
            let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(&p)
                .hidden(false)
                .git_ignore(true)
                .parents(true)
                .build()
                .filter_map(Result::ok)
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.path().to_path_buf())
                .collect();
            files.sort();
            files.truncate(20_000);
            hasher.update(&(files.len() as u64).to_le_bytes());
            for f in files {
                let rel = f
                    .strip_prefix(&p)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .to_string();
                hasher.update(rel.as_bytes());
                hasher.update(&[0]);
                hash_file(hasher, &f);
            }
        } else {
            hasher.update(b"\0absent:");
            hasher.update(&[0]);
        }
    }
}

pub fn task_fingerprint(task: &TaskDef, workdir: &Path) -> String {
    let mut h = blake3::Hasher::new();
    h.update(task.name.as_bytes());
    h.update(&[0]);
    h.update(task.cmd.as_bytes());
    h.update(&[0]);
    if let Some(c) = &task.cwd {
        h.update(c.as_bytes());
    }
    h.update(&[0]);
    let mut outs = task.outputs.clone();
    outs.sort();
    for o in outs {
        h.update(o.as_bytes());
        h.update(&[0]);
    }
    hash_listed(&mut h, workdir, &task.fingerprint);
    h.to_hex().to_string()
}

fn fingerprint_inner(
    tasks: &BTreeMap<String, TaskDef>,
    name: &str,
    workdir: &Path,
    memo: &mut BTreeMap<String, String>,
    stack: &mut Vec<String>,
) -> String {
    if let Some(v) = memo.get(name) {
        return v.clone();
    }
    if stack.iter().any(|s| s == name) {
        return format!("cycle:{name}");
    }
    let Some(task) = tasks.get(name) else {
        return format!("missing:{name}");
    };
    stack.push(name.to_string());
    let mut h = blake3::Hasher::new();
    h.update(task_fingerprint(task, workdir).as_bytes());
    let mut deps = task.deps.clone();
    deps.sort();
    for d in deps {
        let fh = fingerprint_inner(tasks, &d, workdir, memo, stack);
        h.update(fh.as_bytes());
        h.update(&[0]);
    }
    stack.pop();
    let s = h.to_hex().to_string();
    memo.insert(name.to_string(), s.clone());
    s
}

pub fn graph_fingerprints(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
) -> BTreeMap<String, String> {
    let mut memo = BTreeMap::new();
    let mut stack = Vec::new();
    for name in tasks.keys() {
        fingerprint_inner(tasks, name, workdir, &mut memo, &mut stack);
    }
    memo
}

fn journal_path(workdir: &Path) -> PathBuf {
    workdir.join(".keplr/build-journal.json")
}

pub fn load_journal(workdir: &Path) -> BTreeMap<String, JournalEntry> {
    std::fs::read_to_string(journal_path(workdir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_journal(workdir: &Path, journal: &BTreeMap<String, JournalEntry>) -> anyhow::Result<()> {
    let path = journal_path(workdir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(journal)?;
    std::fs::write(&path, text)?;
    Ok(())
}

fn store_outputs(
    cas: &keplr_sync::Cas,
    workdir: &Path,
    task: &TaskDef,
) -> BTreeMap<String, String> {
    let mut stored = BTreeMap::new();
    let mut outs = task.outputs.clone();
    outs.sort();
    for rel in outs {
        let full = resolve_under(workdir, &rel);
        if let Ok(bytes) = std::fs::read(&full) {
            if let Ok(hash) = cas.put(&bytes) {
                stored.insert(rel, hash);
            }
        }
    }
    stored
}

fn restore_outputs(
    cas: &keplr_sync::Cas,
    workdir: &Path,
    entry: &JournalEntry,
) -> Vec<String> {
    let mut restored = Vec::new();
    for (rel, hash) in &entry.outputs {
        let full = resolve_under(workdir, rel);
        if !full.exists() && cas.exists(hash) {
            if let Ok(bytes) = cas.get(hash) {
                if let Some(parent) = full.parent() {
                    if std::fs::create_dir_all(parent).is_err() {
                        continue;
                    }
                }
                if std::fs::write(&full, bytes).is_ok() {
                    restored.push(rel.clone());
                }
            }
        }
    }
    restored
}

fn run_ordered(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
    order: &[String],
    force: bool,
) -> anyhow::Result<Vec<RunReport>> {
    let fps = graph_fingerprints(tasks, workdir);
    let mut journal = load_journal(workdir);
    let cas = keplr_sync::Cas::new(workdir.join(".keplr/cas"));
    let mut reports = Vec::new();
    for name in order {
        let task = tasks
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
        let fp = fps.get(name).cloned().unwrap_or_default();
        let up_to_date = !force && journal.get(name).map(|e| e.hash == fp).unwrap_or(false);
        if up_to_date {
            let restored = restore_outputs(&cas, workdir, &journal[name]);
            let mut output = String::from("up to date");
            if !restored.is_empty() {
                output.push_str(&format!(" (restored {})", restored.join(", ")));
            }
            reports.push(RunReport {
                task: name.clone(),
                skipped: true,
                output,
                hash: fp,
            });
            continue;
        }
        let out = run_task(task, workdir)?;
        let stored = store_outputs(&cas, workdir, task);
        journal.insert(
            name.clone(),
            JournalEntry {
                hash: fp.clone(),
                outputs: stored,
            },
        );
        save_journal(workdir, &journal)?;
        reports.push(RunReport {
            task: name.clone(),
            skipped: false,
            output: out,
            hash: fp,
        });
    }
    Ok(reports)
}

pub fn run_graph(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
    targets: &[String],
    force: bool,
) -> anyhow::Result<Vec<RunReport>> {
    let levels = if targets.is_empty() {
        let all: BTreeSet<String> = tasks.keys().cloned().collect();
        topo_levels(tasks, &all)?
    } else {
        let wanted = closure(tasks, targets)?;
        topo_levels(tasks, &wanted)?
    };
    let order: Vec<String> = levels.into_iter().flatten().collect();
    run_ordered(tasks, workdir, &order, force)
}

pub fn run_graph_parallel(
    tasks: &BTreeMap<String, TaskDef>,
    workdir: &Path,
    targets: &[String],
    jobs: usize,
    force: bool,
) -> anyhow::Result<Vec<RunReport>> {
    let jobs = jobs.clamp(1, 32);
    if jobs == 1 {
        return run_graph(tasks, workdir, targets, force);
    }
    let levels = if targets.is_empty() {
        let all: BTreeSet<String> = tasks.keys().cloned().collect();
        topo_levels(tasks, &all)?
    } else {
        let wanted = closure(tasks, targets)?;
        topo_levels(tasks, &wanted)?
    };
    let fps = graph_fingerprints(tasks, workdir);
    let mut journal = load_journal(workdir);
    let cas = keplr_sync::Cas::new(workdir.join(".keplr/cas"));
    let mut reports = Vec::new();
    for level in levels {
        let mut dirty: Vec<String> = Vec::new();
        for name in &level {
            let fp = fps.get(name).cloned().unwrap_or_default();
            let up_to_date =
                !force && journal.get(name).map(|e| e.hash == fp).unwrap_or(false);
            if up_to_date {
                let restored = restore_outputs(&cas, workdir, &journal[name]);
                let mut output = String::from("up to date");
                if !restored.is_empty() {
                    output.push_str(&format!(" (restored {})", restored.join(", ")));
                }
                reports.push(RunReport {
                    task: name.clone(),
                    skipped: true,
                    output,
                    hash: fp,
                });
            } else {
                dirty.push(name.clone());
            }
        }
        for batch in dirty.chunks(jobs) {
            let batch_out: BTreeMap<String, anyhow::Result<String>> =
                std::thread::scope(|s| {
                    let mut handles = Vec::new();
                    let mut early: BTreeMap<String, anyhow::Result<String>> =
                        BTreeMap::new();
                    for name in batch {
                        let Some(task) = tasks.get(name).cloned() else {
                            early.insert(
                                name.clone(),
                                Err(anyhow::anyhow!("unknown task {name}")),
                            );
                            continue;
                        };
                        let dir = workdir.to_path_buf();
                        handles.push((name.clone(), s.spawn(move || run_task(&task, &dir))));
                    }
                    let mut out = early;
                    for (name, h) in handles {
                        match h.join() {
                            Ok(r) => {
                                out.insert(name, r);
                            }
                            Err(_) => {
                                out.insert(name, Err(anyhow::anyhow!("task `{name}` panicked")));
                            }
                        }
                    }
                    out
                });
            for name in batch {
                match batch_out.get(name) {
                    Some(Ok(text)) => {
                        let task = tasks
                            .get(name)
                            .ok_or_else(|| anyhow::anyhow!("unknown task {name}"))?;
                        let stored = store_outputs(&cas, workdir, task);
                        let fp = fps.get(name).cloned().unwrap_or_default();
                        journal.insert(
                            name.clone(),
                            JournalEntry {
                                hash: fp.clone(),
                                outputs: stored,
                            },
                        );
                        save_journal(workdir, &journal)?;
                        reports.push(RunReport {
                            task: name.clone(),
                            skipped: false,
                            output: text.clone(),
                            hash: fp,
                        });
                    }
                    Some(Err(e)) => {
                        save_journal(workdir, &journal)?;
                        return Err(anyhow::anyhow!("{e:#}"));
                    }
                    None => {
                        save_journal(workdir, &journal)?;
                        anyhow::bail!("task `{name}` produced no result");
                    }
                }
            }
        }
    }
    Ok(reports)
}
