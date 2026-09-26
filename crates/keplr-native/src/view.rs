//! The window as a view tree.
//!
//! Nothing here mutates state or talks to a shell: the view is a pure function
//! of the state, which is what lets a test lay the whole window out with no
//! GPU, no window, and no pty. Node ids are the host's only handle on the tree,
//! so every part the host measures is named.

use std::path::Path;

use keplr_client::Pane;
use keplr_term::{Cell, Snapshot};
use keplr_theme::Theme;
use rcus::{Align, Color, Insets, Justify, Style, ViewNode};

use crate::state::State;

/// Rows of content to draw when the host has not measured the pane yet.
pub const FALLBACK_ROWS: usize = 28;

/// Height of one line of content, in logical pixels.
const LINE: f32 = 18.0;
/// Height of the tab bar.
const TAB_BAR: f32 = 36.0;
/// Height of the status bar.
const STATUS_BAR: f32 = 26.0;
/// Width of the left rail.
const RAIL: f32 = 56.0;
/// Width of the editor's line number gutter, including its right gap.
const GUTTER: f32 = 52.0;

/// The colours the chrome uses, parsed from the shared theme so the native
/// window and the browser client cannot drift apart.
pub struct Chrome {
    pub bg: Color,
    pub raised: Color,
    pub pressed: Color,
    pub text: Color,
    pub secondary: Color,
    pub tertiary: Color,
    pub accent: Color,
    pub ok: Color,
    pub error: Color,
}

impl Chrome {
    /// Parses the AMOLED tokens. A token that does not parse falls back rather
    /// than taking a window down.
    pub fn new(theme: &Theme) -> Self {
        let black = Color::rgba(0.0, 0.0, 0.0, 1.0);
        let white = Color::rgba(1.0, 1.0, 1.0, 1.0);
        Self {
            bg: color(theme.bg, black),
            raised: color(theme.raised, Color::rgba(0.05, 0.05, 0.06, 1.0)),
            pressed: color(theme.pressed, Color::rgba(0.12, 0.12, 0.13, 1.0)),
            text: color(theme.text, white),
            secondary: color(theme.text_secondary, Color::rgba(0.6, 0.6, 0.64, 1.0)),
            tertiary: color(theme.text_tertiary, Color::rgba(0.42, 0.42, 0.46, 1.0)),
            accent: color(theme.accent, Color::rgba(0.04, 0.52, 1.0, 1.0)),
            ok: color(theme.ok, Color::rgba(0.19, 0.82, 0.35, 1.0)),
            error: color(theme.error, Color::rgba(1.0, 0.27, 0.23, 1.0)),
        }
    }
}

/// Parses a `#rrggbb` or `#rgb` token, falling back when it cannot.
pub fn color(token: &str, fallback: Color) -> Color {
    let Some(hex) = token.trim().strip_prefix('#') else {
        return fallback;
    };
    let pair = |index: usize| u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok();
    let channels: Option<Vec<u8>> = match hex.len() {
        // Three digits are shorthand: each one stands for its own pair, so
        // `#f0a` is `#ff00aa`.
        3 => hex
            .chars()
            .map(|digit| u8::from_str_radix(&format!("{digit}{digit}"), 16).ok())
            .collect(),
        6 => (0..3).map(pair).collect(),
        _ => None,
    };
    match channels {
        Some(channels) if channels.len() == 3 => Color::rgba(
            channels[0] as f32 / 255.0,
            channels[1] as f32 / 255.0,
            channels[2] as f32 / 255.0,
            1.0,
        ),
        _ => fallback,
    }
}

/// The whole window: tab bar, rail, showing pane, status bar.
pub fn view(state: &mut State, rows: usize) -> ViewNode {
    let chrome = Chrome::new(&Theme::amoled());
    let status = state.status.clone();
    let tabs: Vec<(String, bool)> = state
        .client
        .tabs()
        .iter()
        .map(|tab| (tab.pane.label(), tab.focused))
        .collect();
    let active = state
        .client
        .active()
        .map(|tab| tab.pane.clone())
        .unwrap_or(Pane::Problems);
    let root = state.client.root().display().to_string();
    let rooms = state.client.rooms().join(" ");
    let content = content(state, rows, &chrome);

    let mut tab_bar = vec![ViewNode::text(
        "keplr",
        Style::default()
            .color(chrome.accent)
            .font_size(14.0)
            .weight(700.0),
    )];
    for (index, (label, focused)) in tabs.iter().enumerate() {
        tab_bar.push(tab(index, label, *focused, &chrome));
    }

    ViewNode::element(
        "window",
        Style::default().background(chrome.bg).fill(true),
        vec![
            ViewNode::row_element(
                "tab-bar",
                Style::default()
                    .height(TAB_BAR)
                    .gap(4.0)
                    .padding(Insets::symmetric(10.0, 0.0))
                    .align(Align::Center)
                    .background(chrome.raised),
                tab_bar,
            ),
            ViewNode::row_element(
                "body",
                Style::default().flex_grow(1.0),
                vec![rail(&active, &chrome), content],
            ),
            ViewNode::row_element(
                "status-bar",
                Style::default()
                    .height(STATUS_BAR)
                    .gap(16.0)
                    .padding(Insets::symmetric(12.0, 0.0))
                    .align(Align::Center)
                    .background(chrome.raised),
                vec![
                    ViewNode::text(
                        root,
                        Style::default().color(chrome.secondary).font_size(11.5),
                    ),
                    ViewNode::text(status, Style::default().color(chrome.ok).font_size(11.5)),
                    ViewNode::text(
                        rooms,
                        Style::default().color(chrome.tertiary).font_size(11.0),
                    ),
                ],
            ),
        ],
    )
}

fn tab(index: usize, label: &str, focused: bool, chrome: &Chrome) -> ViewNode {
    ViewNode::text_node(
        format!("tab-{index}"),
        label.to_string(),
        Style::default()
            .height(24.0)
            .padding(Insets::symmetric(10.0, 0.0))
            .align(Align::Center)
            .background(if focused { chrome.pressed } else { chrome.bg })
            .color(if focused {
                chrome.text
            } else {
                chrome.secondary
            })
            .font_size(12.0),
    )
}

/// The left rail: a fixed column of pane kinds, never a stack of cards.
fn rail(active: &Pane, chrome: &Chrome) -> ViewNode {
    let item = |id: &str, label: &str, glyph: &str, selected: bool| {
        ViewNode::element(
            id.to_string(),
            Style::default()
                .height(48.0)
                .padding(Insets::symmetric(4.0, 6.0))
                .gap(1.0)
                .justify(Justify::Center)
                .align(Align::Center)
                .background(if selected { chrome.pressed } else { chrome.bg })
                .row_height(15.0),
            vec![
                ViewNode::text(
                    glyph.to_string(),
                    Style::default()
                        .color(if selected {
                            chrome.accent
                        } else {
                            chrome.secondary
                        })
                        .font_size(14.0),
                ),
                ViewNode::text(
                    label.to_string(),
                    Style::default()
                        .color(if selected {
                            chrome.text
                        } else {
                            chrome.tertiary
                        })
                        .font_size(9.5),
                ),
            ],
        )
    };
    let (editor, terminal, problems, source) = match active {
        Pane::Editor { .. } => (true, false, false, false),
        Pane::Terminal { .. } => (false, true, false, false),
        Pane::Problems => (false, false, true, false),
        Pane::SourceControl => (false, false, false, true),
    };
    ViewNode::element(
        "rail",
        Style::default()
            .width(RAIL)
            .padding(Insets::symmetric(4.0, 6.0))
            .gap(2.0)
            .background(chrome.raised),
        vec![
            item("rail-editor", "editor", "E", editor),
            item("rail-terminal", "term", "T", terminal),
            item("rail-problems", "issue", "P", problems),
            item("rail-source", "src", "S", source),
        ],
    )
}

/// The showing pane, wrapped so every pane clips its own overflow.
fn content(state: &mut State, rows: usize, chrome: &Chrome) -> ViewNode {
    let pane = state.client.active().map(|tab| tab.pane.clone());
    let body = match pane {
        Some(Pane::Terminal { session }) => terminal(state, &session, chrome),
        Some(Pane::Editor { path, .. }) => editor(state, &path, rows, chrome),
        Some(Pane::Problems) => list(
            "problems",
            "No diagnostics",
            state.diagnostics().to_vec(),
            chrome.error,
            chrome,
        ),
        Some(Pane::SourceControl) => list(
            "source",
            "No tasks reported",
            state.tasks().to_vec(),
            chrome.ok,
            chrome,
        ),
        None => ViewNode::empty(Style::default()),
    };
    ViewNode::element(
        "pane",
        Style::default()
            .flex_grow(1.0)
            .clip(true)
            .background(chrome.bg),
        vec![body],
    )
}

/// A terminal pane, drawn as colour runs of the live grid.
///
/// A run is a maximal span of cells sharing a foreground, a background, and the
/// bold and inverse flags. One text node per run keeps a full grid in the low
/// hundreds of nodes instead of one per cell.
fn terminal(state: &mut State, session: &str, chrome: &Chrome) -> ViewNode {
    let Some(frame) = state.terminal(session) else {
        return empty_pane(
            "terminal",
            "No shell in this pane",
            "ctrl+alt+t starts one",
            chrome,
        );
    };
    let rows = grid_rows(&frame, chrome);
    ViewNode::element(
        "terminal",
        Style::default()
            .flex_grow(1.0)
            .clip(true)
            .padding(Insets::symmetric(10.0, 6.0))
            .row_height(LINE)
            .background(chrome.bg),
        rows,
    )
}

fn grid_rows(frame: &Snapshot, chrome: &Chrome) -> Vec<ViewNode> {
    let mut rows = Vec::with_capacity(frame.cells.len());
    for (y, row) in frame.cells.iter().enumerate() {
        let children = runs(row, frame, y, chrome);
        if children.is_empty() {
            rows.push(ViewNode::empty(
                Style::default().height(LINE).row_height(LINE),
            ));
            continue;
        }
        rows.push(ViewNode::row_element(
            format!("term-row-{y}"),
            Style::default()
                .height(LINE)
                .row_height(LINE)
                .gap(0.0)
                .clip(true),
            children,
        ));
    }
    rows
}

/// Splits one grid row into colour runs, drawing the cursor as an inverse cell.
fn runs(row: &[Cell], frame: &Snapshot, y: usize, chrome: &Chrome) -> Vec<ViewNode> {
    let cursor_here = frame.show_cursor && frame.cursor.line == y as i32;
    let mut nodes: Vec<ViewNode> = Vec::new();
    let mut text = String::new();
    let mut style: Option<Style> = None;
    let mut start = 0usize;
    for (x, cell) in row.iter().enumerate() {
        let on_cursor = cursor_here && frame.cursor.column == x;
        let next = cell_style(cell, on_cursor, chrome);
        if style.as_ref() != Some(&next) {
            if let Some(previous) = style.take() {
                nodes.push(run_node(&text, previous, start, y));
            }
            text.clear();
            start = x;
            style = Some(next);
        }
        text.push_str(&cell.ch);
    }
    if let Some(previous) = style {
        nodes.push(run_node(&text, previous, start, y));
    }
    nodes
}

fn run_node(text: &str, style: Style, x: usize, y: usize) -> ViewNode {
    ViewNode::text_node(format!("term-{y}-{x}"), text.to_string(), style)
}

/// The style of one cell, with the inverse flag and the cursor block folded in.
fn cell_style(cell: &Cell, on_cursor: bool, chrome: &Chrome) -> Style {
    let mut foreground = cell
        .fg
        .as_deref()
        .map(|token| color(token, chrome.text))
        .unwrap_or(chrome.text);
    let mut background = cell
        .bg
        .as_deref()
        .map(|token| color(token, chrome.bg))
        .unwrap_or(chrome.bg);
    if cell.flags & Cell::INVERSE != 0 {
        std::mem::swap(&mut foreground, &mut background);
    }
    if on_cursor {
        background = chrome.accent;
        foreground = chrome.bg;
    }
    Style::default()
        .font_size(13.0)
        .row_height(LINE)
        .color(foreground)
        .background(background)
        .weight(if cell.flags & Cell::BOLD != 0 {
            700.0
        } else {
            400.0
        })
}

/// The byte offset of the caret inside a line, rounded down to a character
/// boundary. The document tracks bytes and the view draws text, so a caret in
/// the middle of a multi-byte character is drawn before that character rather
/// than splitting it.
fn caret_at(text: &str, caret: usize) -> usize {
    let mut at = caret.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// An editor pane: a line number gutter, the text, and a caret block.
///
/// The line holding the cursor is drawn as three pieces, before the caret, the
/// caret itself, and after it, so the block lands where the cursor is rather
/// than only at the end of the line.
fn editor(state: &mut State, path: &Path, rows: usize, chrome: &Chrome) -> ViewNode {
    let Some(document) = state.document(path) else {
        return empty_pane(
            "editor",
            "This file could not be read",
            "ctrl+alt+e opens another",
            chrome,
        );
    };
    let cursor_line = document.cursor_line();
    let caret = document
        .cursor()
        .saturating_sub(document.line_start(cursor_line));
    let total = document.lines();
    let mut children = Vec::with_capacity(rows);
    for offset in 0..rows.max(1) {
        let line = document.top_line() + offset;
        if line >= total {
            break;
        }
        let on_cursor_line = line == cursor_line;
        let text = document.line_text(line).to_string();
        let at = if on_cursor_line {
            caret_at(&text, caret)
        } else {
            text.len()
        };
        let (before, after) = text.split_at(at);
        let body = Style::default()
            .color(chrome.text)
            .font_size(13.0)
            .row_height(LINE);
        children.push(ViewNode::row_element(
            format!("edit-row-{line}"),
            Style::default().height(LINE).row_height(LINE).gap(10.0),
            vec![
                ViewNode::text_node(
                    format!("edit-num-{line}"),
                    (line + 1).to_string(),
                    Style::default()
                        .width(GUTTER - 10.0)
                        .align(Align::End)
                        .color(if on_cursor_line {
                            chrome.secondary
                        } else {
                            chrome.tertiary
                        })
                        .font_size(11.5),
                ),
                ViewNode::text_node(format!("edit-text-{line}"), before.to_string(), body),
                ViewNode::text_node(
                    format!("edit-caret-{line}"),
                    if on_cursor_line {
                        "\u{2588}".to_string()
                    } else {
                        String::new()
                    },
                    Style::default()
                        .color(chrome.accent)
                        .font_size(13.0)
                        .row_height(LINE),
                ),
                ViewNode::text_node(format!("edit-tail-{line}"), after.to_string(), body.clone()),
            ],
        ));
    }
    ViewNode::element(
        "editor",
        Style::default()
            .flex_grow(1.0)
            .clip(true)
            .padding(Insets::symmetric(0.0, 6.0))
            .background(chrome.bg),
        children,
    )
}

/// A list pane, with an empty state that says what would fill it.
fn list(id: &str, empty: &str, items: Vec<String>, tone: Color, chrome: &Chrome) -> ViewNode {
    if items.is_empty() {
        return empty_pane(id, empty, "waiting for the host", chrome);
    }
    ViewNode::element(
        id,
        Style::default()
            .flex_grow(1.0)
            .clip(true)
            .padding(Insets::symmetric(12.0, 8.0))
            .gap(2.0)
            .row_height(LINE)
            .background(chrome.bg),
        items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                ViewNode::text_node(
                    format!("{id}-item-{index}"),
                    item,
                    Style::default()
                        .color(tone)
                        .font_size(12.5)
                        .row_height(LINE),
                )
            })
            .collect(),
    )
}

/// The empty state, which is the point of the pane rather than a gap in it.
fn empty_pane(id: &str, title: &str, hint: &str, chrome: &Chrome) -> ViewNode {
    ViewNode::element(
        id,
        Style::default()
            .flex_grow(1.0)
            .padding(Insets::symmetric(24.0, 24.0))
            .gap(6.0)
            .justify(Justify::Center)
            .background(chrome.bg),
        vec![
            ViewNode::text(
                title.to_string(),
                Style::default().color(chrome.secondary).font_size(13.0),
            ),
            ViewNode::text(
                hint.to_string(),
                Style::default().color(chrome.tertiary).font_size(11.5),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use keplr_term::{Cursor, Snapshot, TerminalGrid};
    use rcus::{App, LayoutNode};
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    fn state() -> State {
        State::new("/tmp", "sh".to_string(), Arc::new(Mutex::new(None)))
    }

    /// Lays the window out for real, with the embedded font, so a test reads the
    /// rectangles and colors the window would actually draw.
    fn layout(state: &mut State) -> App {
        let mut app = App::new(view(state, FALLBACK_ROWS), rcus::fonts::MONO);
        app.resize(1280.0, 800.0);
        app.relayout();
        app
    }

    fn laid_out(nodes: Vec<ViewNode>) -> App {
        App::new(ViewNode::column(nodes), rcus::fonts::MONO)
    }

    fn node<'a>(app: &'a App, id: &str) -> &'a LayoutNode {
        app.layout()
            .root
            .find(id)
            .unwrap_or_else(|| panic!("{id} is in the tree"))
    }

    /// The text a node carries, which is empty for a node that draws nothing.
    fn text_of(app: &App, id: &str) -> String {
        node(app, id).text.clone().unwrap_or_default()
    }

    #[test]
    fn the_window_has_a_bar_a_rail_a_pane_and_a_status() {
        let mut state = state();
        let app = layout(&mut state);
        for id in ["window", "tab-bar", "rail", "pane", "status-bar"] {
            assert!(app.bounds_of(id).is_some(), "{id} is in the tree");
        }
    }

    #[test]
    fn the_status_bar_sits_at_the_bottom_and_the_pane_beside_the_rail() {
        let mut state = state();
        let app = layout(&mut state);
        let status = app.bounds_of("status-bar").expect("status bar");
        assert!(
            (status.bottom() - 800.0).abs() < 0.5,
            "the status bar is flush with the window, got {}",
            status.bottom()
        );
        let rail = app.bounds_of("rail").expect("rail");
        let pane = app.bounds_of("pane").expect("pane");
        assert!(
            (rail.width - RAIL).abs() < 0.5,
            "the rail keeps its width, got {}",
            rail.width
        );
        assert!(
            (rail.right() - pane.x).abs() < 0.5,
            "the pane starts where the rail ends"
        );
        assert!(pane.width > 1000.0, "the pane takes the rest of the row");
    }

    #[test]
    fn the_pane_does_not_run_under_the_tab_bar_or_the_status_bar() {
        let mut state = state();
        let app = layout(&mut state);
        let pane = app.bounds_of("pane").expect("pane");
        let tabs = app.bounds_of("tab-bar").expect("tab bar");
        let status = app.bounds_of("status-bar").expect("status bar");
        assert!(pane.y >= tabs.bottom() - 0.5, "the pane is below the tabs");
        assert!(
            pane.bottom() <= status.y + 0.5,
            "the pane is above the status"
        );
    }

    #[test]
    fn every_open_tab_gets_a_named_node() {
        let mut state = state();
        state.client.open(Pane::SourceControl);
        let app = layout(&mut state);
        assert_eq!(text_of(&app, "tab-0"), "problems");
        assert_eq!(text_of(&app, "tab-1"), "source");
    }

    #[test]
    fn the_focused_tab_is_the_one_the_rail_and_status_agree_on() {
        let mut state = state();
        state.client.open(Pane::SourceControl);
        let app = layout(&mut state);
        assert_eq!(
            node(&app, "tab-1").color,
            Some(Chrome::new(&Theme::amoled()).text),
            "the focused tab is drawn in the reading color"
        );
        assert_eq!(
            node(&app, "rail-source").background,
            Some(Chrome::new(&Theme::amoled()).pressed),
            "the rail marks the showing pane"
        );
    }

    #[test]
    fn a_terminal_pane_draws_one_named_row_per_grid_row() {
        let frame = TerminalGrid::new(20, 4).snapshot();
        let chrome = Chrome::new(&Theme::amoled());
        let app = laid_out(grid_rows(&frame, &chrome));
        for y in 0..4 {
            let row = node(&app, &format!("term-row-{y}"));
            assert!(
                (row.rect.height - LINE).abs() < 0.5,
                "row {y} is one line tall"
            );
            assert!(!row.children.is_empty(), "row {y} has cells");
        }
    }

    #[test]
    fn a_run_of_cells_shares_one_node_and_a_colour_change_starts_another() {
        let cells = vec![
            cell('a', None),
            cell('b', None),
            cell('c', Some("#FF453A")),
            cell('d', Some("#FF453A")),
        ];
        let chrome = Chrome::new(&Theme::amoled());
        let app = laid_out(runs(&cells, &frame_of(4, 1), 0, &chrome));
        assert_eq!(text_of(&app, "term-0-0"), "ab");
        assert_eq!(text_of(&app, "term-0-2"), "cd");
        assert_ne!(
            node(&app, "term-0-0").color,
            node(&app, "term-0-2").color,
            "the red run is not the default color"
        );
    }

    #[test]
    fn the_cursor_cell_is_drawn_as_a_block() {
        let cells = vec![cell('a', None), cell('b', None), cell('c', None)];
        let mut frame = frame_of(3, 1);
        frame.cursor = Cursor { line: 0, column: 1 };
        let chrome = Chrome::new(&Theme::amoled());
        let app = laid_out(runs(&cells, &frame, 0, &chrome));
        assert_eq!(
            node(&app, "term-0-1").background,
            Some(chrome.accent),
            "the cell under the cursor is filled"
        );
        assert_eq!(text_of(&app, "term-0-1"), "b");
    }

    #[test]
    fn a_hidden_cursor_does_not_split_a_run() {
        let cells = vec![cell('a', None), cell('b', None)];
        let mut frame = frame_of(2, 1);
        frame.show_cursor = false;
        frame.cursor = Cursor { line: 0, column: 0 };
        let chrome = Chrome::new(&Theme::amoled());
        let app = laid_out(runs(&cells, &frame, 0, &chrome));
        assert_eq!(text_of(&app, "term-0-0"), "ab");
    }

    #[test]
    fn an_inverse_cell_swaps_its_colors() {
        let mut inverted = cell('x', Some("#FF453A"));
        inverted.flags |= Cell::INVERSE;
        let chrome = Chrome::new(&Theme::amoled());
        let app = laid_out(runs(&[inverted], &frame_of(1, 1), 0, &chrome));
        let node = node(&app, "term-0-0");
        assert_eq!(node.background, Some(chrome.error), "red became the fill");
        assert_ne!(node.color, Some(chrome.error), "and the text is not red");
    }

    #[test]
    fn a_pane_with_nothing_to_show_says_so() {
        let mut state = state();
        let app = layout(&mut state);
        let problems = node(&app, "problems");
        assert_eq!(problems.rect.height, 800.0 - TAB_BAR - STATUS_BAR);
        let mut said_something = false;
        problems.children.iter().for_each(|child| {
            said_something |= child.text.is_some();
        });
        assert!(said_something, "the empty state has words in it");
    }

    #[test]
    fn diagnostics_the_room_reported_become_rows() {
        let mut state = state();
        state.apply_frame(r#"{"kind":"diagnosticsUpdate","path":"src/lib.rs","count":2}"#);
        let app = layout(&mut state);
        assert_eq!(text_of(&app, "problems-item-0"), "src/lib.rs: 2");
    }

    #[test]
    fn a_task_the_room_reported_becomes_a_row() {
        let mut state = state();
        state.apply_frame(r#"{"kind":"taskUpdate","id":"build","state":"passed"}"#);
        state.client.open(Pane::SourceControl);
        let app = layout(&mut state);
        assert_eq!(text_of(&app, "source-item-0"), "build passed");
    }

    #[test]
    fn an_editor_pane_numbers_its_lines_from_one_and_stacks_them() {
        let path = Path::new("/tmp/keplr-native-view-test.txt");
        std::fs::write(path, "first\nsecond\nthird\n").expect("fixture written");
        let mut state = state();
        state.open_editor();
        state.client.open(Pane::Editor {
            path: path.to_path_buf(),
            cursor: 0,
            top_line: 0,
        });
        let app = layout(&mut state);
        let first = node(&app, "edit-row-0");
        let second = node(&app, "edit-row-1");
        assert_eq!(text_of(&app, "edit-num-0"), "1");
        assert_eq!(text_of(&app, "edit-num-1"), "2");
        assert_eq!(text_of(&app, "edit-text-0"), "first");
        assert_eq!(text_of(&app, "edit-text-1"), "second");
        assert_eq!(
            text_of(&app, "edit-caret-1"),
            "",
            "only one line has a caret"
        );
        assert!(
            second.rect.y >= first.rect.bottom() - 0.5,
            "lines stack downwards"
        );
        assert!(first.rect.x >= GUTTER, "text starts after the gutter");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn the_caret_sits_after_the_text_of_the_cursor_line() {
        let path = Path::new("/tmp/keplr-native-caret-test.txt");
        std::fs::write(path, "abc\ndef\n").expect("fixture written");
        let mut state = state();
        state.open_editor();
        state.client.open(Pane::Editor {
            path: path.to_path_buf(),
            cursor: 0,
            top_line: 0,
        });
        state.key(&rcus::InputEvent::KeyDown {
            key: rcus::Key::Character("x".into()),
            modifiers: rcus::Modifiers::default(),
        });
        let app = layout(&mut state);
        // The insert left the cursor between the x and the rest of the line, so
        // the line is drawn in three pieces around the block.
        assert_eq!(text_of(&app, "edit-text-0"), "x");
        assert_eq!(text_of(&app, "edit-caret-0"), "\u{2588}");
        assert_eq!(text_of(&app, "edit-tail-0"), "abc");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_colour_token_that_is_not_hex_falls_back_instead_of_panicking() {
        let black = Color::rgba(0.0, 0.0, 0.0, 1.0);
        let white = Color::rgba(1.0, 1.0, 1.0, 1.0);
        assert_eq!(color("not a colour", black), black);
        assert_eq!(color("#12345", black), black, "five digits is not hex");
        assert_eq!(color("#fff", white), white);
        assert_eq!(
            color("#102030", black),
            Color::rgba(16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0, 1.0)
        );
    }

    fn cell(ch: char, fg: Option<&str>) -> Cell {
        Cell {
            ch: ch.to_string(),
            fg: fg.map(str::to_string),
            bg: None,
            flags: 0,
        }
    }

    /// A frame with the cursor below the last row, so a row is only split when
    /// a test puts the cursor on it.
    fn frame_of(cols: usize, rows: usize) -> Snapshot {
        Snapshot {
            cols,
            rows,
            cursor: Cursor {
                line: rows as i32,
                column: 0,
            },
            show_cursor: true,
            history: Vec::new(),
            cells: vec![vec![cell(' ', None); cols]; rows],
        }
    }
}
