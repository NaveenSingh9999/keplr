use clap::{Parser, Subcommand};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Parser)]
#[command(name = "keplr", version, about = "Keplr personal IDE")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Files {
        query: String,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Search {
        needle: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Open {
        file: PathBuf,
        #[arg(long, default_value_t = 0)]
        line: usize,
    },
    Run {
        task: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long, default_value_t = 1)]
        jobs: usize,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        watch: bool,
    },
    Doctor,
    Serve {
        #[arg(long, default_value_t = 7137)]
        port: u16,
    },
    Ui {
        #[arg(long)]
        open: Option<PathBuf>,
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        palette: Option<String>,
        #[arg(long, default_value_t = 100)]
        width: u16,
    },
    Scene {
        #[arg(long)]
        open: Option<PathBuf>,
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        palette: Option<String>,
        #[arg(long, default_value_t = 100)]
        width: u16,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ws = keplr_core::Workspace::new(cli.root.clone());
    match cli.cmd {
        Cmd::Files { query, limit } => {
            let entries = ws.walk_files(50_000);
            let paths: Vec<PathBuf> = entries.into_iter().map(|e| e.path).collect();
            for p in keplr_core::search::fuzzy_paths(&paths, &query, limit) {
                println!("{}", p.display());
            }
        }
        Cmd::Search { needle, limit } => {
            for hit in ws.grep(&needle, limit) {
                println!("{}:{}:{}: {}", hit.path.display(), hit.line, hit.col, hit.preview);
            }
        }
        Cmd::Open { file, line } => {
            let buf = keplr_core::buffer::Buffer::load(file.clone())?;
            if line == 0 {
                println!("{}", buf.rope);
            } else {
                println!("{}", buf.line(line).unwrap_or_default());
            }
            eprintln!(
                "lang={:?} lines={}",
                keplr_lang::LangKind::from_path(&file),
                buf.len_lines()
            );
        }
        Cmd::Run {
            task,
            all,
            jobs,
            force,
            watch,
        } => {
            let path = cli.root.join("keplr.json");
            let run_once = |force: bool| -> anyhow::Result<Vec<keplr_build::RunReport>> {
                let tasks = keplr_build::load_tasks(&path)?;
                let targets: Vec<String> = match (&task, all) {
                    (_, true) => Vec::new(),
                    (Some(t), false) => vec![t.clone()],
                    (None, false) => Vec::new(),
                };
                if jobs > 1 {
                    keplr_build::run_graph_parallel(&tasks, &cli.root, &targets, jobs, force)
                } else {
                    keplr_build::run_graph(&tasks, &cli.root, &targets, force)
                }
            };
            let print_reports = |reports: &[keplr_build::RunReport]| {
                for r in reports {
                    let state = if r.skipped { "skipped" } else { "ok" };
                    println!("=== {} ({state}) ===", r.task);
                    print!("{}", r.output);
                    if !r.output.ends_with('\n') {
                        println!();
                    }
                }
            };
            if watch {
                println!("keplr: watching {} (Ctrl-C to stop)", cli.root.display());
                let snapshot = || -> anyhow::Result<BTreeMap<String, String>> {
                    let tasks = keplr_build::load_tasks(&path)?;
                    Ok(keplr_build::graph_fingerprints(&tasks, &cli.root))
                };
                let mut last = snapshot()?;
                match run_once(force) {
                    Ok(reports) => print_reports(&reports),
                    Err(e) => eprintln!("keplr: run failed: {e:#}"),
                }
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let now = snapshot()?;
                    if now == last {
                        continue;
                    }
                    last = now;
                    match run_once(false) {
                        Ok(reports) => print_reports(&reports),
                        Err(e) => eprintln!("keplr: run failed: {e:#}"),
                    }
                }
            } else {
                let reports = run_once(force)?;
                print_reports(&reports);
            }
        }
        Cmd::Doctor => {
            println!("root={}", cli.root.display());
            println!("files={}", ws.walk_files(1000).len());
            println!("laml={:?}", keplr_lang::LamlProbe::binary());
        }
        Cmd::Serve { port } => {
            keplr_serve::serve(cli.root, port).await?;
        }
        Cmd::Ui {
            open,
            query,
            palette,
            width,
        } => {
            let mut ui = keplr_ui::UiState::new(cli.root.clone());
            if let Some(path) = open.clone() {
                ui.open_file(path);
            }
            if let Some(q) = palette.clone() {
                ui.palette_open = true;
                ui.palette_query = q;
            } else if !query.is_empty() {
                ui.palette_query = query.clone();
            }
            let scene = ui.to_scene(width);
            let backend = keplr_render::AnsiBackend;
            print!(
                "{}",
                keplr_render::PaintBackend::paint(&backend, &scene, width as usize)
            );
        }
        Cmd::Scene {
            open,
            query,
            palette,
            width,
        } => {
            let scene = keplr_render::build_scene(
                &cli.root,
                open.as_deref(),
                &query,
                palette.as_deref(),
                width,
            );
            println!("{}", serde_json::to_string_pretty(&scene)?);
        }
    }
    Ok(())
}
