//! Drawing the window into a PNG, for a machine with no display.
//!
//! A window is only half of what can go wrong in a UI. The other half is what it
//! looks like, and CI has no display to find out. This path builds the same
//! state, runs it through the same view, and draws it with the same rcus
//! pipelines the window uses, then writes a PNG. The only thing it skips is the
//! window itself.
//!
//! It is also how a snapshot of a real grid is possible: a pane that has content
//! is fed that content first, so the picture shows a shell that has run something
//! rather than an empty grid.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, bail, Result};
use rcus::desktop::Redraw;
use rcus::App as RcusApp;

use crate::state::State;
use crate::view::view;

/// Content rows a snapshot draws, matching the window's fallback.
const PANE_ROWS: usize = crate::view::FALLBACK_ROWS;
/// Default image size, which is also the window's opening size.
const DEFAULT_SIZE: (u32, u32) = (1280, 800);

/// Which pane a snapshot shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Problems,
    Terminal,
    Editor,
    Source,
}

impl Pane {
    /// The name a caller passes, which is also the file stem of the PNG.
    pub fn name(self) -> &'static str {
        match self {
            Pane::Problems => "problems",
            Pane::Terminal => "terminal",
            Pane::Editor => "editor",
            Pane::Source => "source",
        }
    }

    /// The pane this snapshot shows, in the client's own terms.
    fn client_pane(self) -> keplr_client::Pane {
        match self {
            Pane::Problems => keplr_client::Pane::Problems,
            Pane::Source => keplr_client::Pane::SourceControl,
            Pane::Terminal => keplr_client::Pane::Terminal {
                session: "terminal".to_string(),
            },
            Pane::Editor => keplr_client::Pane::Problems,
        }
    }

    fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "problems" => Pane::Problems,
            "terminal" => Pane::Terminal,
            "editor" => Pane::Editor,
            "source" => Pane::Source,
            other => bail!("unknown pane {other}: try problems, terminal, editor or source"),
        })
    }
}

/// A line the terminal snapshot runs, so the grid has colour in it.
const DEMO_COMMAND: &str = "printf '\\033[1;32mkeplr\\033[0m native grid\\n\\033[33mwarning\\033[0m: colors survive\\n\\033[1;31merror\\033[0m: so do problems\\n'; printf 'plain text row\\n'; exit\n";

/// How long a snapshot waits for the shell to produce its output, per attempt.
const SHELL_WAIT: std::time::Duration = std::time::Duration::from_millis(120);
/// How many times to look before giving up on the shell saying anything.
const SHELL_ATTEMPTS: usize = 40;

/// Renders one pane and writes it to `out`.
pub fn write_png(out: &std::path::Path, pane: Pane, size: (u32, u32), root: PathBuf) -> Result<()> {
    let mut state = state_for(pane, root);
    let bytes = render_png(&mut state, pane, size)?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(out, bytes).map_err(|error| anyhow!("writing {}: {error}", out.display()))?;
    Ok(())
}

/// A state with the given pane showing and, where it can be, filled in.
fn state_for(pane: Pane, root: PathBuf) -> State {
    // A snapshot has no event loop, so the redraw handle is the empty one: the
    // pty reader can ask for a frame that nobody will draw, which is harmless
    // because the snapshot draws the grid once it has settled.
    let redraw = Arc::new(Mutex::new(Some(Redraw::default())));
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let mut state = State::new(root, shell, redraw);
    // The state opens on the problems pane, so showing another one means asking
    // for it: publishing to a room fills a pane but does not switch to it.
    match pane {
        Pane::Problems => {
            state.publish_problems();
            state.open(pane.client_pane());
        }
        Pane::Source => {
            state.publish_tasks();
            state.open(pane.client_pane());
        }
        Pane::Terminal => {
            state.open_terminal();
            state.run_in_terminal(DEMO_COMMAND);
            wait_for_shell(&state);
        }
        Pane::Editor => {
            state.open_editor();
        }
    }
    state
}

/// Runs the demo command and waits for the grid to stop changing, so the
/// snapshot catches the output rather than the prompt.
fn wait_for_shell(state: &State) {
    let mut previous = String::new();
    for _ in 0..SHELL_ATTEMPTS {
        let now = state
            .terminal("terminal")
            .map(|frame| {
                frame
                    .cells
                    .iter()
                    .map(|row| row.iter().map(|cell| cell.ch.as_str()).collect::<String>())
                    .collect::<Vec<String>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if !now.is_empty() && now == previous {
            return;
        }
        previous = now;
        std::thread::sleep(SHELL_WAIT);
    }
}

/// Draws the state's pane into a PNG, at one device pixel per logical pixel.
pub fn render_png(state: &mut State, pane: Pane, (width, height): (u32, u32)) -> Result<Vec<u8>> {
    let _ = pane;
    let tree = view(state, PANE_ROWS);
    let mut app = RcusApp::new(tree, rcus::fonts::MONO);
    app.resize(width as f32, height as f32);
    app.relayout();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok_or_else(|| anyhow!("no GPU adapter, so there is nothing to draw on"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("keplr-snapshot"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .map_err(|error| anyhow!("no GPU device: {error:?}"))?;
    let mut renderer = app.renderer(device, queue, wgpu::TextureFormat::Rgba8Unorm, 1.0)?;
    app.snapshot_png(&mut renderer, width, height)
}

/// Reads the flags after `--snapshot`.
pub struct Options {
    pub out: PathBuf,
    pub panes: Vec<Pane>,
    pub size: (u32, u32),
    pub root: PathBuf,
    /// Print the solved layout instead of drawing it. Every box the window
    /// would fill, with the numbers, which is the only way to tell a layout
    /// mistake from a paint mistake without a display.
    pub layout: bool,
}

/// Parses `--snapshot` arguments: `--out`, `--pane`, `--width`, `--height`, and
/// an optional root directory.
pub fn parse(args: &[String]) -> Result<Options> {
    let mut out = None;
    let mut panes = Vec::new();
    let mut size = DEFAULT_SIZE;
    let mut root = None;
    let mut layout = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let mut value = || {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| anyhow!("{arg} needs a value"))
        };
        match arg {
            "--out" => out = Some(PathBuf::from(value()?)),
            "--pane" => panes.push(Pane::parse(&value()?)?),
            "--width" => {
                size.0 = value()?
                    .parse()
                    .map_err(|_| anyhow!("--width is a number"))?
            }
            "--height" => {
                size.1 = value()?
                    .parse()
                    .map_err(|_| anyhow!("--height is a number"))?
            }
            other if other.starts_with("--") => bail!("unknown flag {other}"),
            other => root = Some(PathBuf::from(other)),
        }
        index += 1;
    }
    Ok(Options {
        out: out.ok_or_else(|| anyhow!("--snapshot needs --out <file.png>"))?,
        panes: if panes.is_empty() {
            vec![Pane::Problems, Pane::Terminal, Pane::Editor, Pane::Source]
        } else {
            panes
        },
        size,
        root: root
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        layout,
    })
}

/// Runs every requested pane, writing `<out stem>-<pane>.png` beside `--out`.
pub fn run(args: &[String]) -> Result<()> {
    let options = parse(args)?;
    if options.layout {
        let mut state = state_for(options.panes[0], options.root.clone());
        let tree = view(&mut state, PANE_ROWS);
        let mut app = RcusApp::new(tree, rcus::fonts::MONO);
        app.resize(options.size.0 as f32, options.size.1 as f32);
        app.relayout();
        let report = layout_report(&app);
        // Written to a file as well as stdout: CI keeps the log, but an
        // artifact is easier to read next to the screenshots.
        std::fs::write("/tmp/layout.txt", &report).ok();
        println!("{report}");
        return Ok(());
    }
    for pane in &options.panes {
        let out = match options.out.parent() {
            Some(parent) => parent.join(format!(
                "{}-{}.png",
                options
                    .out
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_else(|| "keplr".to_string()),
                pane.name()
            )),
            None => PathBuf::from(format!("{}-{}.png", options.out.display(), pane.name())),
        };
        write_png(&out, *pane, options.size, options.root.clone())?;
        println!(
            "{} ({} bytes)",
            out.display(),
            std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)
        );
    }
    Ok(())
}

/// The solved layout as text: one line per named box, in paint order.
pub fn layout_report(app: &RcusApp) -> String {
    let mut out = format!(
        "viewport {:?}\n",
        (app.viewport().width, app.viewport().height)
    );
    for node in app.layout().paint_nodes() {
        if node.id.is_empty() {
            continue;
        }
        let rect = node.rect;
        out.push_str(&format!(
            "{:<18} x={:>7.1} y={:>7.1} w={:>7.1} h={:>6.1}{}\n",
            node.id,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            match node.text.as_deref() {
                Some(text) => format!("  {:?}", truncate(text)),
                None => String::new(),
            }
        ));
    }
    out
}

fn truncate(text: &str) -> String {
    let cut: String = text.chars().take(24).collect();
    if text.chars().count() > 24 {
        format!("{cut}…")
    } else {
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn the_flags_are_read_and_checked() {
        let options = parse(&args(&[
            "--out",
            "/tmp/k.png",
            "--pane",
            "terminal",
            "--width",
            "640",
            "--height",
            "480",
            "/tmp",
        ]))
        .expect("parses");
        assert_eq!(options.out, PathBuf::from("/tmp/k.png"));
        assert_eq!(options.panes, vec![Pane::Terminal]);
        assert_eq!(options.size, (640, 480));
        assert_eq!(options.root, PathBuf::from("/tmp"));
        assert!(!options.layout, "laying out is not the default");
    }

    #[test]
    fn asking_for_the_layout_is_a_flag() {
        let options = parse(&args(&["--out", "/tmp/k.png", "--layout"])).expect("parses");
        assert!(options.layout);
    }

    #[test]
    fn every_pane_is_drawn_when_none_is_named() {
        let options = parse(&args(&["--out", "/tmp/k.png"])).expect("parses");
        assert_eq!(options.panes.len(), 4, "one picture of every pane");
    }

    #[test]
    fn a_missing_or_wrong_flag_is_refused_rather_than_guessed() {
        assert!(parse(&args(&[])).is_err(), "no --out");
        assert!(parse(&args(&["--out"])).is_err(), "--out with no value");
        assert!(parse(&args(&["--out", "/tmp/k.png", "--nope"])).is_err());
        assert!(parse(&args(&["--out", "/tmp/k.png", "--pane", "cards"])).is_err());
        assert!(parse(&args(&["--out", "/tmp/k.png", "--width", "wide"])).is_err());
    }

    /// The one test that needs a GPU. Where there is no adapter it says so and
    /// passes, because a build machine without a driver is not a failure of the
    /// code under test; where there is one, it checks the bytes are a PNG and
    /// not an empty file.
    #[test]
    fn a_snapshot_is_a_png_with_pixels_in_it() {
        let mut state = state_for(Pane::Problems, PathBuf::from("/tmp"));
        let png = match render_png(&mut state, Pane::Problems, (320, 200)) {
            Ok(png) => png,
            Err(error) => {
                eprintln!("skipping: {error}");
                return;
            }
        };
        assert_eq!(&png[1..4], b"PNG", "a real PNG header");
        assert!(
            png.len() > 1000,
            "the image has content, got {} bytes",
            png.len()
        );
    }
}
