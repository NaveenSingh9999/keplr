use clap::{Parser, Subcommand};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

mod tui;

#[derive(Parser)]
#[command(name = "keplr", version, about = "Keplr personal IDE")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum GitCmd {
    Status,
    Log {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Branches,
    Diff,
    Commit {
        #[arg(short, long)]
        message: String,
    },
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
        #[arg(long, default_value = "walk")]
        via: String,
    },
    Index {
        #[arg(long)]
        refresh: bool,
    },
    Watch {
        #[arg(long, default_value_t = 500)]
        debounce_ms: u64,
    },
    Bench {
        #[arg(long, default_value_t = 1000)]
        files: usize,
        #[arg(long, default_value_t = 40)]
        lines: usize,
        #[arg(long)]
        reuse: bool,
        #[arg(long)]
        json: bool,
    },
    Open {
        file: PathBuf,
        #[arg(long, default_value_t = 0)]
        line: usize,
    },
    Edit {
        file: PathBuf,
        #[arg(long)]
        no_animations: bool,
    },
    Save {
        file: PathBuf,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        stdin: bool,
        #[arg(long)]
        task: Option<String>,
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
    Diagnostics {
        file: PathBuf,
    },
    LspInstall {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    Snippets {
        lang: String,
        prefix: Option<String>,
    },
    Git {
        #[command(subcommand)]
        cmd: GitCmd,
    },
    Serve {
        #[arg(long, default_value_t = 7137)]
        port: u16,
        #[arg(long, default_value = "")]
        token: String,
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        #[arg(long)]
        allow_open_lan: bool,
    },
    Token {
        #[arg(long)]
        save: bool,
        #[arg(long)]
        rotate: bool,
    },
    Ui {
        #[arg(long)]
        open: Option<PathBuf>,
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        palette: Option<String>,
        #[arg(long, default_value = "files")]
        palette_mode: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value = "project")]
        left_tab: String,
        #[arg(long, default_value = "symbols")]
        right_tab: String,
        #[arg(long, default_value = "terminal")]
        bottom_tab: String,
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
        #[arg(long, default_value = "files")]
        palette_mode: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value = "project")]
        left_tab: String,
        #[arg(long, default_value = "symbols")]
        right_tab: String,
        #[arg(long, default_value = "terminal")]
        bottom_tab: String,
        #[arg(long, default_value_t = 100)]
        width: u16,
    },
    #[cfg(feature = "desktop")]
    Desktop {
        #[arg(long)]
        open: Option<PathBuf>,
        #[arg(long, default_value = "")]
        query: String,
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
        Cmd::Search { needle, limit, via } => {
            let hits = match via.as_str() {
                "index" => {
                    let mut index = keplr_core::Index::load(&ws);
                    if index.is_empty() {
                        index = keplr_core::Index::build(&ws);
                        let _ = index.save(&ws);
                    }
                    index.grep(&needle, limit)
                }
                "trigram" => ws.grep_trigram(&needle, limit),
                _ => ws.grep(&needle, limit),
            };
            for hit in hits {
                println!("{}:{}:{}: {}", hit.path.display(), hit.line, hit.col, hit.preview);
            }
        }
        Cmd::Index { refresh } => {
            let mut index = keplr_core::Index::load(&ws);
            if refresh && !index.is_empty() {
                let changed = index.refresh(&ws);
                index.save(&ws)?;
                println!(
                    "index files={} changed={} path={}",
                    index.len(),
                    changed.len(),
                    ws.root.join(".keplr/index.json").display()
                );
            } else {
                index = keplr_core::Index::build(&ws);
                index.save(&ws)?;
                println!(
                    "index files={} path={}",
                    index.len(),
                    ws.root.join(".keplr/index.json").display()
                );
            }
        }
        Cmd::Watch { debounce_ms } => {
            let mut index = keplr_core::Index::load(&ws);
            if index.is_empty() {
                index = keplr_core::Index::build(&ws);
                index.save(&ws)?;
            }
            println!("watching {} (Ctrl-C to stop)", cli.root.display());
            loop {
                let changes = keplr_core::poll_changes(&cli.root, debounce_ms)?;
                if changes.is_empty() {
                    continue;
                }
                for c in &changes {
                    index.apply(&ws, &c.path);
                    println!("{:?} {}", c.kind, c.path.display());
                }
                index.save(&ws)?;
            }
        }
        Cmd::Bench {
            files,
            lines,
            reuse,
            json,
        } => {
            let corpus = cli.root.join(".bench-corpus");
            if !reuse || !corpus.exists() {
                let _ = std::fs::remove_dir_all(&corpus);
                std::fs::create_dir_all(&corpus)?;
            }
            let t0 = std::time::Instant::now();
            let made = if reuse && corpus.exists() {
                keplr_core::Workspace::new(corpus.clone()).walk_files(100_000).len()
            } else {
                keplr_core::synth_tree(&corpus, files, lines)?.len()
            };
            let gen_ms = t0.elapsed().as_millis();
            let bws = keplr_core::Workspace::new(corpus.clone());
            let t0 = std::time::Instant::now();
            let walked = bws.walk_files(100_000).len();
            let walk_ms = t0.elapsed().as_millis();
            let t0 = std::time::Instant::now();
            let bindex = {
                let b = keplr_core::Index::build(&bws);
                let _ = b.save(&bws);
                b
            };
            let index_ms = t0.elapsed().as_millis();
            let queries = ["serve", "config", "fn alpha", "route", "zzzz_no_match_zzz"];
            let mut fuzzy_ns = Vec::new();
            let mut grep_ns = Vec::new();
            for _ in 0..4 {
                for q in queries {
                    let t = std::time::Instant::now();
                    let paths: Vec<PathBuf> =
                        bindex.files().iter().map(|e| e.path.clone()).collect();
                    let _ = keplr_core::search::fuzzy_paths(&paths, q, 10);
                    fuzzy_ns.push(t.elapsed().as_nanos());
                    let t = std::time::Instant::now();
                    let _ = bws.grep_trigram(q, 10);
                    grep_ns.push(t.elapsed().as_nanos());
                }
            }
            let t0 = std::time::Instant::now();
            let cas = keplr_sync::Cas::new(bws.cas_dir());
            let blob = vec![7u8; 4096];
            let mut puts = 0;
            while t0.elapsed().as_millis() < 500 {
                let _ = cas.put(&blob)?;
                puts += 1;
            }
            let cas_ms = t0.elapsed().as_millis().max(1);
            let bench_json = cli.root.join(".bench-corpus/keplr.json");
            std::fs::write(
                &bench_json,
                r#"{"tasks":{"gen":{"cmd":"echo gen","outputs":[]},"wrap":{"cmd":"echo wrap","outputs":[],"deps":["gen"]}}}"#,
            )?;
            let btasks = keplr_build::load_tasks(&bench_json)?;
            let r1 = keplr_build::run_graph(&btasks, &corpus, &[], false)?;
            let r2 = keplr_build::run_graph(&btasks, &corpus, &[], false)?;
            let skipped = r2.iter().filter(|r| r.skipped).count();
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "files_made": made,
                        "files_walked": walked,
                        "gen_ms": gen_ms,
                        "walk_ms": walk_ms,
                        "index_ms": index_ms,
                        "fuzzy_p50_us": keplr_core::percentile_ns(&fuzzy_ns, 50.0) / 1000,
                        "fuzzy_p95_us": keplr_core::percentile_ns(&fuzzy_ns, 95.0) / 1000,
                        "grep_p50_us": keplr_core::percentile_ns(&grep_ns, 50.0) / 1000,
                        "grep_p95_us": keplr_core::percentile_ns(&grep_ns, 95.0) / 1000,
                        "cas_put_per_s": (puts as u128 * 1000) / cas_ms,
                        "build_rerun": r1.len(),
                        "build_skipped": skipped,
                    }))?
                );
            } else {
                println!(
                    "bench files_made={} walked={} gen_ms={} walk_ms={} index_ms={}",
                    made, walked, gen_ms, walk_ms, index_ms
                );
                println!(
                    "fuzzy_p50_us={} fuzzy_p95_us={} grep_p50_us={} grep_p95_us={}",
                    keplr_core::percentile_ns(&fuzzy_ns, 50.0) / 1000,
                    keplr_core::percentile_ns(&fuzzy_ns, 95.0) / 1000,
                    keplr_core::percentile_ns(&grep_ns, 50.0) / 1000,
                    keplr_core::percentile_ns(&grep_ns, 95.0) / 1000
                );
                println!(
                    "cas_put_per_s={} build_rerun={} build_skipped={}",
                    (puts as u128 * 1000) / cas_ms,
                    r1.len(),
                    skipped
                );
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
        Cmd::Edit { file, no_animations } => {
            tui::edit_file(cli.root, file, no_animations)?;
        }
        Cmd::Save {
            file,
            content,
            stdin,
            task,
        } => {
            use std::io::Read;
            let text = if let Some(c) = content {
                c
            } else if stdin {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s)?;
                s
            } else {
                anyhow::bail!("save needs --content STR or --stdin");
            };
            let report = keplr_core::save_buffer(&ws, &file, &text)?;
            println!(
                "saved {} bytes={} hash={} cas={} index_files={} git={}",
                report.path,
                report.bytes,
                report.hash,
                report.cas_stored,
                report.index_files,
                report.git_committed
            );
            if !report.git_output.trim().is_empty() {
                println!("git: {}", report.git_output.lines().next().unwrap_or_default());
            }
            if let Some(t) = task {
                let tasks = keplr_build::load_tasks(&cli.root.join("keplr.json"))?;
                let reports = keplr_build::run_graph(&tasks, &cli.root, &[t], false)?;
                for r in &reports {
                    let state = if r.skipped { "skipped" } else { "ok" };
                    println!("=== {} ({state}) ===", r.task);
                    print!("{}", r.output);
                }
            }
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
        Cmd::Diagnostics { file } => {
            let lang = keplr_lang::LangKind::from_path(&file);
            let diagnostics = if lang == keplr_lang::LangKind::Laml {
                keplr_lang::laml_diagnostics(&file)
            } else {
                Vec::new()
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "file": file.display().to_string(),
                    "lang": format!("{lang:?}"),
                    "diagnostics": diagnostics,
                    "servers": keplr_lang::lsp_servers(lang),
                }))?
            );
        }
        Cmd::LspInstall { name, dir } => {
            if name == "rust-analyzer" {
                let dest = dir.unwrap_or_else(|| cli.root.join(".keplr/bin"));
                let bin = keplr_lang::install_rust_analyzer(&dest)?;
                println!("installed to {}", bin.display());
            } else {
                let present = keplr_lang::command_present(&name);
                println!("present={present} hint: {}", keplr_lang::install_hint(&name));
            }
        }
        Cmd::Snippets { lang, prefix } => {
            let kind = keplr_lang::LangKind::from_path(Path::new(&format!("x.{lang}")));
            match prefix {
                Some(p) => match keplr_lang::expand_snippet(kind, &p) {
                    Some((text, cursor)) => {
                        println!("{text}");
                        if let Some(c) = cursor {
                            eprintln!("cursor={c}");
                        }
                    }
                    None => anyhow::bail!("no snippet `{p}` for {lang}"),
                },
                None => {
                    for s in keplr_lang::snippets_for(kind) {
                        println!("{} — {}", s.prefix, s.description);
                    }
                }
            }
        }
        Cmd::Git { cmd } => match cmd {
            GitCmd::Status => {
                for e in keplr_core::git::status(&cli.root)? {
                    println!("{}{} {}", e.staged, e.unstaged, e.path);
                }
            }
            GitCmd::Log { limit } => {
                for e in keplr_core::git::log(&cli.root, limit)? {
                    println!("{} {} {} {}", &e.hash[..8.min(e.hash.len())], e.date, e.author, e.message);
                }
            }
            GitCmd::Branches => {
                for b in keplr_core::git::branches(&cli.root)? {
                    let mark = if b.current { "*" } else { " " };
                    println!("{mark} {}", b.name);
                }
            }
            GitCmd::Diff => {
                print!("{}", keplr_core::git::diff_stat(&cli.root)?);
            }
            GitCmd::Commit { message } => {
                print!("{}", keplr_core::git::commit(&cli.root, &message)?);
            }
        },
        Cmd::Serve {
            port,
            token,
            bind,
            allow_open_lan,
        } => {
            let resolved = keplr_serve::resolve_token(&cli.root, &token);
            if resolved.is_empty() {
                eprintln!("keplr: no token configured — serving open on 127.0.0.1");
            } else {
                eprintln!("keplr: token gate enabled");
            }
            keplr_serve::serve_full(cli.root, port, resolved, &bind, allow_open_lan).await?;
        }
        Cmd::Token { save, rotate } => {
            let token = keplr_serve::new_token();
            if rotate || save {
                let dir = cli.root.join(".keplr");
                std::fs::create_dir_all(&dir)?;
                let path = dir.join("token");
                std::fs::write(&path, format!("{token}\n"))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &path,
                        std::fs::Permissions::from_mode(0o600),
                    );
                }
                if rotate {
                    println!("rotated: {}", path.display());
                } else {
                    println!("saved to {}", path.display());
                }
            } else {
                println!("{token}");
            }
        }
        Cmd::Ui {
            open,
            query,
            palette,
            palette_mode,
            search,
            left_tab,
            right_tab,
            bottom_tab,
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
            ui.set_palette_mode(&palette_mode);
            if let Some(q) = search.clone() {
                ui.search_query = q;
            }
            ui.set_left_tab(&left_tab);
            ui.set_right_tab(&right_tab);
            ui.set_bottom_tab(&bottom_tab);
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
            palette_mode,
            search,
            left_tab,
            right_tab,
            bottom_tab,
            width,
        } => {
            let spec = keplr_render::SceneSpec {
                root: &cli.root,
                open_file: open.as_deref(),
                query: &query,
                palette_query: palette.as_deref(),
                palette_mode: &palette_mode,
                search_query: search.as_deref(),
                left_tab: &left_tab,
                right_tab: &right_tab,
                bottom_tab: &bottom_tab,
                width,
            };
            let scene = keplr_render::build_scene(&spec);
            println!("{}", serde_json::to_string_pretty(&scene)?);
        }
        #[cfg(feature = "desktop")]
        Cmd::Desktop { open, query } => {
            match keplr_render::gpu::run_desktop(cli.root.clone(), open.clone(), query.clone())
            {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("keplr: gpu unavailable ({e:#}); software fallback");
                    let mut ui = keplr_ui::UiState::new(cli.root.clone());
                    if let Some(path) = open.clone() {
                        ui.open_file(path);
                    }
                    if !query.is_empty() {
                        ui.palette_query = query.clone();
                    }
                    let scene = ui.to_scene(100);
                    print!(
                        "{}",
                        keplr_render::PaintBackend::paint(
                            &keplr_render::AnsiBackend,
                            &scene,
                            100
                        )
                    );
                }
            }
        }
    }
    Ok(())
}
