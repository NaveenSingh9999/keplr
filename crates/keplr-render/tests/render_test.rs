use keplr_render::{build_scene, diff_scenes, AnsiBackend, PaintBackend, Theme};
use std::path::Path;

#[test]
fn theme_scene_paint_and_diff_are_real() {
    let t = Theme::amoled();
    assert!(t.bg.starts_with('#'));
    assert!(t.text.starts_with('#'));

    let root_buf = std::env::temp_dir().join("keplr-render-test");
    let root = root_buf.as_path();
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\nline2\n").unwrap();
    std::fs::write(root.join("README.md"), "hello\n").unwrap();

    let spec = keplr_render::SceneSpec {
        root,
        open_file: Some(Path::new("src/main.rs")),
        query: "main",
        palette_query: None,
        palette_mode: "files",
        search_query: None,
        left_tab: "project",
        right_tab: "symbols",
        bottom_tab: "terminal",
        width: 100,
    };
    let scene = build_scene(&spec);
    assert!(scene.center.path.contains("main.rs"));
    assert_eq!(scene.center.lang, "rust");
    assert!(!scene.left.lines.is_empty());
    assert!(scene.status.files >= 2);

    let painted = AnsiBackend.paint(&scene, 80);
    assert!(painted.contains("main.rs"));
    assert!(painted.len() > 100);

    let mut other = scene.clone();
    other.center.cursor = (2, 1);
    let ops = diff_scenes(&scene, &other);
    assert!(ops.iter().any(|op| op.path == "center.cursor"));
    let same = diff_scenes(&scene, &scene.clone());
    assert!(same.is_empty());
}
