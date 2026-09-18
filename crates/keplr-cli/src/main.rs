use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        task: String,
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
        Cmd::Run { task } => {
            let tasks = keplr_build::load_tasks(&cli.root.join("keplr.json"))?;
            let def = tasks.get(&task).ok_or_else(|| anyhow::anyhow!("unknown task {task}"))?;
            print!("{}", keplr_build::run_task(def, &cli.root)?);
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
