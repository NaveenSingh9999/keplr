use keplr_core::{search::fuzzy_paths, Workspace};
use std::{fs, path::PathBuf};

#[test]
fn fuzzy_and_grep_find_expected() {
    let paths = vec![
        PathBuf::from("src/editor.rs"),
        PathBuf::from("src/search.rs"),
        PathBuf::from("README.md"),
    ];
    let got = fuzzy_paths(&paths, "edr", 5);
    assert_eq!(got, vec![PathBuf::from("src/editor.rs")]);

    let root = PathBuf::from("/tmp/keplr-grep-test");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.txt"), "hello keplr\nsecond line\n").unwrap();
    let ws = Workspace::new(root);
    let hits = ws.grep("keplr", 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[0].col, 7);
}
