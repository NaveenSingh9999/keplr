//! The window frame: title bar, activity bar, sidebar, editor area, panel and
//! status bar.
//!
//! Everything here is a full-bleed band that reaches the window edge. Nothing
//! floats, nothing is inset, and no band is a rounded card: a window has exactly
//! one grid, and the bands are its rows and columns. The only rounded thing in
//! the shell is the one thing that floats, which is nothing.

use std::path::Path;

use rcus::{Align, Insets, Justify, Style, ViewNode};

use crate::state::{Row, SidebarView, State};
use crate::theme::Chrome;

/// How tall each band is. macOS and VS Code agree closely enough that one set of
/// numbers makes both feel right.
pub const TITLE_BAR_H: f32 = 38.0;
pub const TAB_STRIP_H: f32 = 35.0;
pub const ACTIVITY_BAR_W: f32 = 48.0;
pub const STATUS_BAR_H: f32 = 24.0;

/// The sidebar's width when nothing has dragged it, and the range it can be
/// dragged within.
pub const SIDEBAR_DEFAULT: f32 = 260.0;
pub const SIDEBAR_MIN: f32 = 200.0;
pub const SIDEBAR_MAX: f32 = 520.0;

/// The panel's height when it opens, and the smallest it can be dragged to.
pub const PANEL_DEFAULT: f32 = 240.0;
pub const PANEL_MIN: f32 = 120.0;

/// The activity bar's four entries, top to bottom.
pub const ACTIVITY_VIEWS: [SidebarView; 4] = [
    SidebarView::Files,
    SidebarView::Search,
    SidebarView::Source,
    SidebarView::Outline,
];

/// The whole window.
pub fn view(state: &mut State, rows: usize) -> ViewNode {
    let chrome = state.chrome();
    let root = state.client.root().to_path_buf();
    let active = state.sidebar_view();

    let sidebar = sidebar(state, active, &chrome);
    let editor_area = ViewNode::element(
        "editor-area",
        Style::default()
            .flex_grow(1.0)
            .clip(true)
            .background(chrome.chrome),
        vec![ViewNode::column(vec![crate::view::pane(
            state, rows, &chrome,
        )])],
    );
    let editor_side = ViewNode::column(vec![
        tab_strip(state, &chrome),
        editor_area,
        panel(state, &chrome),
    ]);

    ViewNode::column(vec![
        title_bar(&root, &chrome),
        ViewNode::row_element(
            "window",
            Style::default()
                .flex_grow(1.0)
                .clip(true)
                .background(chrome.chrome),
            vec![activity_bar(active, &chrome), sidebar, editor_side],
        ),
        status_bar(state, &chrome),
    ])
}

/// The top band: the app on the left, the window's own title in the middle, and
/// nothing else. No menu bar of fake menus, no pills.
fn title_bar(root: &Path, chrome: &Chrome) -> ViewNode {
    let title = root
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "keplr".to_string());
    ViewNode::element(
        "title-bar",
        Style::default()
            .height(TITLE_BAR_H)
            .padding(Insets::symmetric(14.0, 0.0))
            .align(Align::Center)
            .background(chrome.surface)
            .color(chrome.text),
        vec![
            ViewNode::text_node(
                "title-app",
                "keplr",
                Style::default()
                    .font_size(12.0)
                    .weight(600.0)
                    .color(chrome.text_faint),
            ),
            ViewNode::element(
                "title-name",
                Style::default()
                    .flex_grow(1.0)
                    .align(Align::Center)
                    .justify(Justify::Center),
                vec![ViewNode::text_node(
                    "title-label",
                    title,
                    Style::default()
                        .font_size(12.0)
                        .weight(600.0)
                        .color(chrome.text),
                )],
            ),
            ViewNode::text_node(
                "title-hint",
                "keplr",
                Style::default().font_size(12.0).color(chrome.text_faint),
            ),
        ],
    )
}

/// The left rail. One glyph per view, no text, no rounded container: the active
/// one is drawn in the accent colour and that is the whole indicator.
fn activity_bar(active: SidebarView, chrome: &Chrome) -> ViewNode {
    let mut children = Vec::new();
    for candidate in ACTIVITY_VIEWS {
        let on = candidate == active;
        let colour = if on { chrome.accent } else { chrome.text_faint };
        children.push(ViewNode::element(
            format!("activity-{}", candidate.slug()),
            Style::default()
                .height(46.0)
                .align(Align::Center)
                .justify(Justify::Center),
            vec![ViewNode::text_node(
                format!("activity-glyph-{}", candidate.slug()),
                candidate.glyph().to_string(),
                Style::default().font_size(17.0).color(colour),
            )],
        ));
    }
    ViewNode::element(
        "activity-bar",
        Style::default()
            .width(ACTIVITY_BAR_W)
            .padding(Insets::symmetric(0.0, 6.0))
            .align(Align::Center)
            .background(chrome.chrome),
        children,
    )
}

/// The sidebar: a header, then the rows, flush to the left edge. Only the
/// workspace name is bold, because it is the only one that is a header.
fn sidebar(state: &mut State, active: SidebarView, chrome: &Chrome) -> ViewNode {
    let title = match active {
        SidebarView::Files => "explorer",
        SidebarView::Search => "search",
        SidebarView::Source => "source control",
        SidebarView::Outline => "outline",
    };

    let mut children = vec![ViewNode::element(
        "sidebar-header",
        Style::default()
            .height(30.0)
            .padding(Insets::symmetric(14.0, 0.0))
            .align(Align::Center),
        vec![ViewNode::text_node(
            "sidebar-title",
            title.to_uppercase(),
            Style::default()
                .font_size(10.0)
                .weight(700.0)
                .color(chrome.text_faint),
        )],
    )];

    if active == SidebarView::Files {
        for (index, row) in state.sidebar_rows().iter().enumerate() {
            children.push(sidebar_row(index, row, chrome));
        }
    } else {
        let empty = match active {
            SidebarView::Search => "No results",
            SidebarView::Source => "No changes",
            SidebarView::Outline => "No symbols",
            SidebarView::Files => "",
        };
        children.push(ViewNode::element(
            "sidebar-empty",
            Style::default()
                .padding(Insets::symmetric(14.0, 10.0))
                .font_size(12.0)
                .color(chrome.text_faint),
            vec![ViewNode::text(
                empty,
                Style::default().font_size(12.0).color(chrome.text_faint),
            )],
        ));
    }

    ViewNode::element(
        "sidebar",
        Style::default()
            .width(state.sidebar_width())
            .padding(Insets::symmetric(0.0, 6.0))
            .clip(true)
            .background(chrome.surface),
        children,
    )
}

/// One sidebar row. Nested rows get less indent and a fainter glyph, which is
/// the only cue the depth needs.
fn sidebar_row(index: usize, row: &Row, chrome: &Chrome) -> ViewNode {
    let nested = index > 0;
    let indent = if nested { 26.0 } else { 12.0 };
    let colour = if nested {
        chrome.text_faint
    } else {
        chrome.text
    };
    let weight = if nested { 400.0 } else { 600.0 };
    ViewNode::element(
        format!("sidebar-row-{}", index),
        Style::default()
            .height(24.0)
            .padding(Insets::symmetric(indent, 0.0))
            .align(Align::Center)
            .gap(8.0),
        vec![
            ViewNode::text_node(
                format!("sidebar-glyph-{}", index),
                row.glyph.to_string(),
                Style::default().font_size(12.0).color(colour),
            ),
            ViewNode::text_node(
                format!("sidebar-label-{}", index),
                row.label.clone(),
                Style::default()
                    .font_size(12.5)
                    .weight(weight)
                    .color(colour),
            ),
        ],
    )
}

/// The tab strip above the editor. Flat, one hairline under it, active tab in
/// the window colour with an accent bar.
fn tab_strip(state: &mut State, chrome: &Chrome) -> ViewNode {
    let mut children = Vec::new();
    for (index, tab) in state.client.tabs().iter().enumerate() {
        let focused = tab.focused;
        let colour = if focused {
            chrome.text
        } else {
            chrome.text_faint
        };
        let mut style = Style::default()
            .height(TAB_STRIP_H)
            .padding(Insets::symmetric(14.0, 0.0))
            .align(Align::Center)
            .gap(8.0)
            .font_size(12.5)
            .color(colour);
        style = if focused {
            style.background(chrome.chrome)
        } else {
            style.background(chrome.surface)
        };
        children.push(ViewNode::element(
            format!("tab-{}", index),
            style,
            vec![ViewNode::text_node(
                format!("tab-label-{}", index),
                tab.pane.label(),
                Style::default()
                    .font_size(12.5)
                    .weight(if focused { 600.0 } else { 400.0 })
                    .color(colour),
            )],
        ));
    }
    if children.is_empty() {
        children.push(ViewNode::element(
            "tab-empty",
            Style::default()
                .height(TAB_STRIP_H)
                .padding(Insets::symmetric(14.0, 0.0))
                .align(Align::Center)
                .background(chrome.surface),
            vec![ViewNode::text(
                "no tabs",
                Style::default().font_size(12.0).color(chrome.text_faint),
            )],
        ));
    }
    ViewNode::row_element(
        "tab-strip",
        Style::default()
            .height(TAB_STRIP_H)
            .background(chrome.surface),
        children,
    )
}

/// The bottom panel. Closed it is nothing at all: no band, no border, no
/// reserved space.
fn panel(state: &mut State, chrome: &Chrome) -> ViewNode {
    if !state.panel_open() {
        return ViewNode::empty(Style::default().height(0.0).flex_shrink(0.0));
    }
    ViewNode::element(
        "panel",
        Style::default()
            .height(state.panel_height())
            .flex_shrink(0.0)
            .padding(Insets::symmetric(12.0, 8.0))
            .clip(true)
            .background(chrome.surface),
        vec![ViewNode::text(
            "panel",
            Style::default().font_size(12.0).color(chrome.text_faint),
        )],
    )
}

/// The bottom band, always present, always 24 tall.
fn status_bar(state: &mut State, chrome: &Chrome) -> ViewNode {
    let left = state.client.root().display().to_string();
    let right = if state.panel_open() {
        "panel open"
    } else {
        "ctrl+j panel"
    };
    ViewNode::element(
        "status-bar",
        Style::default()
            .height(STATUS_BAR_H)
            .padding(Insets::symmetric(12.0, 0.0))
            .align(Align::Center)
            .background(chrome.surface),
        vec![
            ViewNode::element(
                "status-left",
                Style::default().flex_grow(1.0),
                vec![ViewNode::text(
                    left,
                    Style::default().font_size(11.0).color(chrome.text_faint),
                )],
            ),
            ViewNode::text_node(
                "status-right",
                right,
                Style::default().font_size(11.0).color(chrome.text_faint),
            ),
        ],
    )
}
