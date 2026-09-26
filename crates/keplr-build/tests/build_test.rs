use keplr_build::{load_tasks, run_task};

#[test]
fn load_and_run_echo_task() {
    let path = std::env::temp_dir().join("keplr-tasks-test.json");
    std::fs::write(
        &path,
        r#"{"tasks":{"hi":{"cmd":"echo hello","outputs":[]}}}"#,
    )
    .unwrap();
    let tasks = load_tasks(&path).unwrap();
    assert!(tasks.contains_key("hi"));
    let out = run_task(&tasks["hi"], &std::env::temp_dir()).unwrap();
    assert!(out.contains("hello"));
}
