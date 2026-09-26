//! Keplr's native window.
//!
//! One process, one window: the [`State`] decides what should be on screen, the
//! view turns that into a tree, and rcus draws it. Keystrokes come back as input
//! events, and a shell writing output comes back as a redraw request, which
//! arrives as a tick on this thread and rebuilds the tree.
//!
//! With `--snapshot` it opens no window at all: the same state and the same view
//! are drawn once into a PNG. That is how CI checks what the window looks like
//! on a machine with no display.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rcus::desktop::Redraw;
use rcus::{App as RcusApp, DesktopApp, InputEvent};

mod snapshot;
mod state;
mod view;

use state::State;
use view::{view, FALLBACK_ROWS};

/// Content rows in the showing pane. The pane is not measured against the window
/// yet, so the shell is started at a fixed grid instead: the number still has to
/// match what the pane draws, or the grid and the rows disagree.
const PANE_ROWS: usize = FALLBACK_ROWS;
/// Columns the first shell is started with, for the same reason.
const PANE_COLS: usize = 100;

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // `--snapshot` draws the same window into a PNG and opens nothing, which is
    // how CI and a machine with no display see the UI at all.
    if argv.first().map(String::as_str) == Some("--snapshot") {
        return snapshot::run(&argv[1..]);
    }
    let mut args = argv.into_iter();
    let root = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());

    // The window's redraw handle only exists once the rcus app is built, which
    // needs a view, which needs the state. The state therefore lives behind a
    // cell, and the handle is installed into its slot before the loop starts, so
    // no shell can be started before it has somewhere to draw.
    let redraw: Arc<Mutex<Option<Redraw>>> = Arc::new(Mutex::new(None));
    let state = Rc::new(RefCell::new(State::new(root, shell, redraw)));
    let root_view = view(&mut state.borrow_mut(), PANE_ROWS);

    let handler = Rc::clone(&state);
    let window = DesktopApp::new("Keplr", RcusApp::new(root_view, rcus::fonts::MONO)).on_input(
        move |event, root, _layout| {
            let mut state = handler.borrow_mut();
            let changed = match event {
                // A shell wrote output, or the host published to a room this
                // window watches: the model moved, so the window is rebuilt.
                InputEvent::Tick => {
                    state.drain_events();
                    state.resize_terminal(PANE_COLS, PANE_ROWS);
                    true
                }
                InputEvent::KeyDown { .. } => state.key(&event),
                _ => false,
            };
            if changed {
                *root = view(&mut state, PANE_ROWS);
            }
        },
    );

    state.borrow_mut().install_redraw(window.redraw());
    window.run()
}

/// Draws one frame into a PNG instead of opening a window.
///
/// This is the same state, the same view, and the same rcus pipelines the window
/// uses, so a CI job on a machine with no GPU or no display can still see what
/// the window would show. Each pane is rendered separately, because a snapshot
/// of an empty problems list says nothing about a full one.
#[cfg(test)]
mod snapshot {}

/// Renders the given state into `out`, sized `width` by `height`.
///
/// Kept behind a function so a test can drive it and so the binary path stays a
/// single call.
pub fn render_png(state: &mut State, width: u32, height: u32) -> Result<Vec<u8>> {
    use std::sync::Arc as StdArc;

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
    .ok_or_else(|| anyhow::anyhow!("no GPU adapter, so there is nothing to draw on"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("keplr-snapshot"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .map_err(|error| anyhow::anyhow!("no GPU device: {error:?}"))?;
    let mut renderer = app.renderer(device, queue, wgpu::TextureFormat::Rgba8Unorm, 1.0)?;
    let _ = StdArc::new(());
    app.snapshot_png(&mut renderer, width, height)
}
