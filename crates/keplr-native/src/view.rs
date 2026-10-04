//! The window as a view tree.
//!
//! Nothing here mutates state or talks to a shell: the view is a pure function
//! of the state, which is what lets a test lay the whole window out with no
//! GPU, no window, and no pty. Node ids are the host's only handle on the tree,
//! so every part the host measures is named.

use std::path::Path;

use keplr_client::Pane;
use keplr_lang::{highlight, LangKind, TokenKind};
use keplr_term::{Cell, Snapshot};
use rcus::{Color, Insets, Justify, Style, ViewNode};

use crate::state::State;
use crate::theme::Chrome;

/// Rows of content to draw when the host has not measured the pane yet.
pub const FALLBACK_ROWS: usize = 28;

/// Height of one line of content, in logical pixels.
const LINE: f32 = 18.0;
/// Height of the tab bar.
/// Height of the status bar.
/// Width of the left rail.
/// Width of the editor's line number gutter, including its right gap.
const GUTTER: f32 = 52.0;

/// The window. The shell builds the frame and this supplies the pane inside it.
pub fn view(state: &mut State, rows: usize) -> ViewNode {
    crate::shell::view(state, rows)
}

/// The pane the editor area is showing: whatever the focused tab holds, or
/// nothing when there is no tab.
pub fn pane(state: &mut State, rows: usize, chrome: &Chrome) -> ViewNode {
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
            chrome.success,
            chrome,
        ),
        None => ViewNode::empty(Style::default()),
    };
    ViewNode::element(
        "pane",
        Style::default()
            .flex_grow(1.0)
            .clip(true)
            .background(chrome.chrome),
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
            .background(chrome.chrome),
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
        .map(|token| chrome.colour(token, chrome.text))
        .unwrap_or(chrome.text);
    let mut background = cell
        .bg
        .as_deref()
        .map(|token| chrome.colour(token, chrome.chrome))
        .unwrap_or(chrome.chrome);
    if cell.flags & Cell::INVERSE != 0 {
        std::mem::swap(&mut foreground, &mut background);
    }
    if on_cursor {
        background = chrome.accent;
        foreground = chrome.chrome;
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
    let lang = LangKind::from_path(path);
    let cursor_line = document.cursor_line();
    let selection = document.selection();
    let total = document.lines();
    let mut children = Vec::with_capacity(rows);
    for offset in 0..rows.max(1) {
        let line = document.top_line() + offset;
        if line >= total {
            break;
        }
        let on_cursor_line = line == cursor_line;
        let text = document.line_text(line).to_string();
        children.push(ViewNode::row_element(
            format!("edit-row-{line}"),
            Style::default()
                .height(LINE)
                .row_height(LINE)
                .gap(10.0)
                .background(if on_cursor_line {
                    chrome.current_line
                } else {
                    // A transparent fill keeps the text rows from showing a
                    // background only where a token claims one.
                    Color::rgba(0.0, 0.0, 0.0, 0.0)
                }),
            editor_row(
                line,
                &text,
                lang,
                document.line_start(line),
                selection,
                on_cursor_line,
                if on_cursor_line {
                    document.cursor() - document.line_start(line)
                } else {
                    usize::MAX
                },
                chrome,
            ),
        ));
    }
    ViewNode::element(
        "editor",
        Style::default()
            .flex_grow(1.0)
            .clip(true)
            .padding(Insets::symmetric(0.0, 6.0))
            .background(chrome.chrome),
        children,
    )
}

/// One editable line, drawn as colour runs with the caret and the selection
/// laid out as if the selection were text.
fn editor_row(
    line: usize,
    text: &str,
    lang: LangKind,
    line_byte: usize,
    selection: Option<(usize, usize)>,
    on_cursor_line: bool,
    caret_col: usize,
    chrome: &Chrome,
) -> Vec<ViewNode> {
    let mut runs = Vec::new();
    let ws_len = text
        .bytes()
        .take_while(|b| *b == b' ' || *b == b'\t')
        .count();
    if ws_len > 0 {
        runs.push(Run {
            text: String::new(),
            kind: TokenKind::Other,
            selected: selection
                .map(|(start, end)| line_byte < end && line_byte + ws_len > start)
                .unwrap_or(false),
            guided: true,
            guides: guided_indent(&text[..ws_len]),
            start: line_byte,
        });
    }
    for run in highlight_line(lang, text, line_byte, selection, ws_len) {
        runs.push(Run {
            guided: false,
            ..run
        });
    }

    let mut row = Vec::new();
    // The line number stays its own node, before the runs.
    row.push(ViewNode::text_node(
        format!("edit-num-{line}"),
        (line + 1).to_string(),
        Style::default()
            .width(GUTTER - 10.0)
            .color(if on_cursor_line {
                chrome.text_muted
            } else {
                chrome.text_faint
            })
            .font_size(11.5),
    ));
    let mut caret_drawn = false;
    for (index, run) in runs.iter().enumerate() {
        let style = token_style(run.kind, run.selected, chrome);
        if run.guided {
            let guide_style = Style::default()
                .color(chrome.indent_guide)
                .font_size(13.0)
                .row_height(LINE);
            let guide_style = if run.selected {
                guide_style.background(chrome.selection)
            } else {
                guide_style
            };
            // The caret takes precedence over a column in the indent, so the
            // guide splits around it rather than letting it hide.
            if on_cursor_line && caret_col < run.guides.chars().count() {
                let split = caret_col.min(run.guides.len());
                let (before, after) = run.guides.split_at(split.min(run.guides.len()));
                row.push(ViewNode::text_node(
                    format!("edit-indent-{}a", line),
                    before.to_string(),
                    guide_style.clone(),
                ));
                row.push(ViewNode::text_node(
                    format!("edit-caret-{}", line),
                    "\u{2588}".to_string(),
                    Style::default()
                        .color(chrome.accent)
                        .font_size(13.0)
                        .row_height(LINE),
                ));
                row.push(ViewNode::text_node(
                    format!("edit-indent-{}b", line),
                    after.to_string(),
                    guide_style,
                ));
                caret_drawn = true;
                continue;
            }
            row.push(ViewNode::text_node(
                format!("edit-indent-{}", line),
                run.guides.clone(),
                guide_style,
            ));
            continue;
        }
        // A run owns the caret while the caret is strictly inside it; a caret
        // between two runs belongs to the one it opens, so only one of them
        // ever splits.
        let run_start = run.start - line_byte;
        let run_end = run_start + run.text.len();
        if on_cursor_line && !caret_drawn && caret_col >= run_start && caret_col < run_end {
            let split = (caret_col - run_start).min(run.text.len());
            let (before, after) = run.text.split_at(split);
            row.push(ViewNode::text_node(
                format!("edit-run-{}-{}a", line, index),
                before.to_string(),
                style.clone(),
            ));
            row.push(ViewNode::text_node(
                format!("edit-caret-{}", line),
                "\u{2588}".to_string(),
                Style::default()
                    .color(chrome.accent)
                    .font_size(13.0)
                    .row_height(LINE),
            ));
            row.push(ViewNode::text_node(
                format!("edit-run-{}-{}b", line, index),
                after.to_string(),
                style,
            ));
            caret_drawn = true;
            continue;
        }
        row.push(ViewNode::text_node(
            format!("edit-run-{}-{}", line, index),
            run.text.clone(),
            style,
        ));
    }
    // A caret at the end of the last token, or on an empty line, owns no run.
    if on_cursor_line && !caret_drawn {
        row.push(ViewNode::text_node(
            format!("edit-caret-{}", line),
            "\u{2588}".to_string(),
            Style::default()
                .color(chrome.accent)
                .font_size(13.0)
                .row_height(LINE),
        ));
    }
    row
}

/// The colour and background for a token, with the selection painted over the
/// token's own background.
fn token_style(kind: TokenKind, selected: bool, chrome: &Chrome) -> Style {
    let color = match kind {
        TokenKind::Keyword => chrome.syntax_keyword,
        TokenKind::Str => chrome.syntax_string,
        TokenKind::Comment => chrome.syntax_comment,
        TokenKind::Number => chrome.syntax_number,
        TokenKind::Type => chrome.syntax_type,
        TokenKind::Function => chrome.syntax_function,
        TokenKind::Macro => chrome.syntax_macro,
        TokenKind::Attribute => chrome.syntax_function,
        TokenKind::Constant => chrome.syntax_constant,
        TokenKind::Parameter => chrome.syntax_variable,
        TokenKind::Punctuation => chrome.text_faint,
        TokenKind::Other => chrome.text,
    };
    let style = Style::default()
        .color(color)
        .font_size(13.0)
        .row_height(LINE);
    if selected {
        style.background(chrome.selection)
    } else {
        style
    }
}

/// The leading whitespace as vertical bars at every indent step: `│   │   `
/// for two groups of four. The bars are the guides, the spaces keep the columns
/// aligned, and the bars are the only thing the indent colour is drawn over.
fn guided_indent(whitespace: &str) -> String {
    let cols: usize = whitespace
        .chars()
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    let mut out = String::with_capacity(cols);
    let mut col = 0;
    while col < cols {
        out.push('\u{2502}');
        out.extend(std::iter::repeat(' ').take(3));
        col += 4;
    }
    out
}

/// The highlight runs for the text after the guided indent, each split at
/// selection boundaries so a selection that starts mid-token only covers the
/// part it owns.
fn highlight_line(
    lang: LangKind,
    text: &str,
    line_byte: usize,
    selection: Option<(usize, usize)>,
    ws_len: usize,
) -> Vec<Run> {
    let spans = highlight(lang, text);
    let sel = selection.unwrap_or((usize::MAX, usize::MAX));
    let mut runs = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut current_kind = TokenKind::Other;
    let mut current_selected = false;

    // Re-tag every byte with its token kind, falling back to Other.
    let mut kinds = vec![TokenKind::Other; text.len()];
    for span in spans {
        let end = (span.start + span.len).min(text.len());
        for byte in span.start..end {
            kinds[byte] = span.kind;
        }
    }

    for byte in ws_len..text.len() {
        let selected = line_byte + byte >= sel.0 && line_byte + byte < sel.1;
        if current_start.is_some() && kinds[byte] == current_kind && selected == current_selected {
            continue;
        }
        if let Some(start) = current_start.take() {
            runs.push(Run {
                text: text[start..byte].to_string(),
                kind: current_kind,
                selected: current_selected,
                guided: false,
                guides: String::new(),
                start: line_byte + start,
            });
        }
        current_start = Some(byte);
        current_kind = kinds[byte];
        current_selected = selected;
    }
    if let Some(start) = current_start {
        runs.push(Run {
            text: text[start..].to_string(),
            kind: current_kind,
            selected: current_selected,
            guided: false,
            guides: String::new(),
            start: line_byte + start,
        });
    }
    runs
}

/// The span of text the guide has to paint.
struct Run {
    text: String,
    kind: TokenKind,
    selected: bool,
    guided: bool,
    guides: String,
    start: usize,
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
            .background(chrome.chrome),
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
            .background(chrome.chrome),
        vec![
            ViewNode::text(
                title.to_string(),
                Style::default().color(chrome.text_muted).font_size(13.0),
            ),
            ViewNode::text(
                hint.to_string(),
                Style::default().color(chrome.text_faint).font_size(11.5),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::{
        ACTIVITY_BAR_W as RAIL, STATUS_BAR_H as STATUS_BAR, TAB_STRIP_H as TAB_BAR,
    };
    use crate::state::State;
    use crate::theme::Chrome;
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

    fn state_chrome() -> Chrome {
        Chrome::new(&keplr_theme::UserTheme::default())
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
        for id in [
            "window",
            "title-bar",
            "tab-strip",
            "activity-bar",
            "sidebar",
            "pane",
            "status-bar",
        ] {
            assert!(app.bounds_of(id).is_some(), "{id} is in the tree");
        }
    }

    #[test]
    fn the_bars_span_the_whole_window_rather_than_sitting_in_the_middle() {
        // `align` places a node inside its parent, so a bar that wants to be
        // full width must not carry one: it ends up sized to its content and
        // centred, which looks like a deliberate floating panel and is not.
        let mut state = state();
        let app = layout(&mut state);
        for id in ["title-bar", "status-bar", "window"] {
            let rect = app.bounds_of(id).unwrap_or_else(|| panic!("{id}"));
            assert!(
                (rect.width - 1280.0).abs() < 0.5,
                "{id} is {} wide, expected the full window",
                rect.width
            );
            assert!(rect.x.abs() < 0.5, "{id} starts at {}", rect.x);
        }
    }

    #[test]
    fn a_rail_item_fills_the_rail() {
        let mut state = state();
        let app = layout(&mut state);
        let rail = app.bounds_of("activity-bar").expect("activity bar");
        for id in [
            "activity-files",
            "activity-search",
            "activity-source",
            "activity-outline",
        ] {
            let item = app.bounds_of(id).unwrap_or_else(|| panic!("{id}"));
            assert!(
                item.width >= rail.width - 9.0,
                "{id} is {} wide inside a {} wide rail",
                item.width,
                rail.width
            );
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
        let rail = app.bounds_of("activity-bar").expect("activity bar");
        let sidebar = app.bounds_of("sidebar").expect("sidebar");
        let pane = app.bounds_of("pane").expect("pane");
        assert!(
            (rail.width - RAIL).abs() < 0.5,
            "the rail keeps its width, got {}",
            rail.width
        );
        assert!(
            (rail.right() - sidebar.x).abs() < 0.5,
            "the sidebar starts where the rail ends"
        );
        assert!(
            (sidebar.right() - pane.x).abs() < 0.5,
            "the pane starts where the sidebar ends"
        );
        assert!(pane.width > 800.0, "the pane takes the rest of the row");
    }

    #[test]
    fn the_pane_does_not_run_under_the_tab_bar_or_the_status_bar() {
        let mut state = state();
        let app = layout(&mut state);
        let pane = app.bounds_of("pane").expect("pane");
        let tabs = app.bounds_of("tab-strip").expect("tab strip");
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
        assert_eq!(text_of(&app, "tab-label-0"), "problems");
        assert_eq!(text_of(&app, "tab-label-1"), "source");
    }

    #[test]
    fn the_focused_tab_is_the_one_the_rail_and_status_agree_on() {
        let mut state = state();
        state.client.open(Pane::SourceControl);
        let app = layout(&mut state);
        assert_eq!(
            node(&app, "tab-label-1").color,
            Some(state_chrome().text),
            "the focused tab is drawn in the reading color"
        );
        assert_eq!(
            node(&app, "activity-glyph-files").color,
            Some(state_chrome().accent),
            "the rail marks the view the sidebar is showing"
        );
    }

    #[test]
    fn a_terminal_pane_draws_one_named_row_per_grid_row() {
        let frame = TerminalGrid::new(20, 4).snapshot();
        let chrome = state_chrome();
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
        let chrome = state_chrome();
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
        let chrome = state_chrome();
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
        let chrome = state_chrome();
        let app = laid_out(runs(&cells, &frame, 0, &chrome));
        assert_eq!(text_of(&app, "term-0-0"), "ab");
    }

    #[test]
    fn an_inverse_cell_swaps_its_colors() {
        let mut inverted = cell('x', Some("#FF453A"));
        inverted.flags |= Cell::INVERSE;
        let chrome = state_chrome();
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
        assert_eq!(
            problems.rect.height,
            800.0 - crate::shell::TITLE_BAR_H - TAB_BAR - STATUS_BAR
        );
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
        // The cursor starts at the top of the file, so line 0 is drawn as an
        // empty run, the caret, and then the rest of the line.
        assert_eq!(text_of(&app, "edit-run-0-0a"), "");
        assert_eq!(text_of(&app, "edit-caret-0"), "\u{2588}");
        assert_eq!(text_of(&app, "edit-run-0-0b"), "first");
        assert_eq!(text_of(&app, "edit-run-1-0"), "second");
        assert!(
            second.rect.y >= first.rect.bottom() - 0.5,
            "lines stack downwards"
        );
        let num = node(&app, "edit-num-0");
        let first_run = node(&app, "edit-run-0-0b");
        assert!(
            first_run.rect.x > num.rect.x,
            "text starts after the gutter number"
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn an_indented_line_paints_guides() {
        let fixture = std::env::temp_dir().join("keplr-native-indent-test.rs");
        std::fs::write(&fixture, "    fn main() {}\n").expect("fixture written");
        let mut state = state();
        state.open_editor();
        state.client.open(Pane::Editor {
            path: fixture.clone(),
            cursor: 0,
            top_line: 0,
        });
        let app = layout(&mut state);
        let guides = text_of(&app, "edit-indent-0");
        assert!(
            guides.contains('\u{2502}'),
            "the indent step draws a guide bar, got {guides:?}"
        );
        std::fs::remove_file(&fixture).ok();
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
        assert_eq!(text_of(&app, "edit-run-0-0a"), "x");
        assert_eq!(text_of(&app, "edit-caret-0"), "\u{2588}");
        assert_eq!(text_of(&app, "edit-run-0-0b"), "abc");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_colour_token_that_is_not_hex_falls_back_instead_of_panicking() {
        let black = Color::rgba(0.0, 0.0, 0.0, 1.0);
        let white = Color::rgba(1.0, 1.0, 1.0, 1.0);
        let chrome = state_chrome();
        assert_eq!(chrome.colour("not a colour", black), black);
        assert_eq!(
            chrome.colour("#12345", black),
            black,
            "five digits is not hex"
        );
        assert_eq!(chrome.colour("#fff", white), white);
        assert_eq!(
            chrome.colour("#102030", black),
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
