use keplr_core::Workspace;
use std::fs;

#[test]
fn walk_skips_git_target_and_caps_limit() {
    let root = std::env::temp_dir().join("keplr-walk-test");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("src/a.rs"), "fn a() {}").unwrap();
    fs::write(root.join("src/b.rs"), "fn b() {}").unwrap();
    fs::write(root.join(".git/x"), "y").unwrap();
    fs::write(root.join("target/z"), "w").unwrap();
    let ws = Workspace::new(root);
    let all = ws.walk_files(100);
    assert_eq!(all.len(), 2);
    let one = ws.walk_files(1);
    assert_eq!(one.len(), 1);
}
