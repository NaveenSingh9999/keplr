use keplr_ui::{Dock, UiState, key_action};
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

    assert!(matches!(key_action("ctrl+p"), keplr_ui::Action::OpenPalette));
    assert!(matches!(key_action("ctrl+b"), keplr_ui::Action::ToggleLeft));
    assert!(matches!(
        key_action("f12-unknown-xyz"),
        keplr_ui::Action::Unknown
    ));

    ui.push_terminal(String::from("build ok"));
    assert!(
        ui.to_scene(100)
            .bottom
            .lines
            .iter()
            .any(|l| l.contains("build ok"))
    );
}
