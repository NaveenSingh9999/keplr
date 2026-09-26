use keplr_ui::{
    key_action, Dock, LayoutError, PaneAxis, PaneCard, PaneKind, UiState, WorkbenchLayout,
};
use std::path::PathBuf;

#[test]
fn ui_tabs_docks_palette_and_keymap_are_real() {
    let root = std::env::temp_dir().join("keplr-ui-test");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(root.join("src/b.rs"), "fn b() {}\n").unwrap();

    let mut ui = UiState::new(root.clone());
    assert!(ui.left_visible);
    ui.open_file(PathBuf::from("src/a.rs"));
    ui.open_file(PathBuf::from("src/b.rs"));
    assert_eq!(ui.tabs.len(), 2);
    assert!(ui.active_editor().unwrap().path.ends_with("src/b.rs"));

    ui.toggle(Dock::Left);
    assert!(!ui.left_visible);
    ui.toggle(Dock::Left);
    assert!(ui.left_visible);

    ui.palette_query = String::from("a.rs");
    let hits = ui.palette_results(5);
    assert!(hits.iter().any(|p| p.ends_with("src/a.rs")));

    let scene = ui.to_scene(100);
    assert!(scene.center.path.contains("b.rs"));
    assert_eq!(scene.status.files, 2);
    assert!(scene.layout.is_some());

    assert!(matches!(
        key_action("ctrl+p"),
        keplr_ui::Action::OpenPalette
    ));
    assert!(matches!(key_action("ctrl+b"), keplr_ui::Action::ToggleLeft));
    assert!(matches!(
        key_action("f12-unknown-xyz"),
        keplr_ui::Action::Unknown
    ));

    ui.push_terminal(String::from("build ok"));
    assert!(ui
        .to_scene(100)
        .bottom
        .lines
        .iter()
        .any(|l| l.contains("build ok")));
}

#[test]
fn workbench_default_preserves_the_current_dock_shape() {
    let layout = WorkbenchLayout::current_default();

    assert_eq!(
        layout.tree.leaf_ids(),
        vec!["left", "editor", "right", "bottom"]
    );
    assert!(layout.tree.leaf("left").unwrap().visible);
    assert!(!layout.tree.leaf("right").unwrap().visible);
    assert!(!layout.tree.leaf("bottom").unwrap().visible);
    assert_eq!(layout.focused_leaf, "editor");
}

#[test]
fn workbench_splits_and_moves_cards_between_leaves() {
    let mut layout = WorkbenchLayout::current_default();
    let terminal = PaneCard::new("terminal-1", PaneKind::Terminal, "Terminal");

    let terminal_leaf = layout
        .tree
        .split_leaf("editor", PaneAxis::Horizontal, terminal.clone())
        .unwrap();

    assert_ne!(terminal_leaf, "editor");
    assert_eq!(
        layout.tree.leaf(&terminal_leaf).unwrap().cards[0].id,
        terminal.id
    );

    layout.tree.move_card("terminal-1", "right").unwrap();
    assert!(!layout
        .tree
        .leaf("editor")
        .unwrap()
        .cards
        .iter()
        .any(|card| card.id == "terminal-1"));
    assert!(layout
        .tree
        .leaf("right")
        .unwrap()
        .cards
        .iter()
        .any(|card| card.id == "terminal-1"));
    assert!(layout.tree.leaf("right").unwrap().visible);
    assert!(matches!(
        layout.tree.move_card("terminal-1", "missing"),
        Err(LayoutError::UnknownLeaf(_))
    ));
}

#[test]
fn workbench_layout_round_trips_as_json() {
    let mut layout = WorkbenchLayout::current_default();
    layout
        .tree
        .split_leaf(
            "editor",
            PaneAxis::Vertical,
            PaneCard::new("files-2", PaneKind::Files, "Files"),
        )
        .unwrap();

    let encoded = serde_json::to_string(&layout).unwrap();
    let decoded: WorkbenchLayout = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded, layout);
    assert_eq!(decoded.version, WorkbenchLayout::current_default().version);
}
