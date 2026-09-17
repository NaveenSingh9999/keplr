use std::process::Command;

#[test]
fn help_lists_commands() {
    let out = Command::new(env!("CARGO_BIN_EXE_keplr"))
        .arg("--help")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("search"));
    assert!(text.contains("serve"));
}
