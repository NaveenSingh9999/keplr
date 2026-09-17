use std::{collections::BTreeMap, path::Path};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDef {
    pub name: String,
    pub cmd: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
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
