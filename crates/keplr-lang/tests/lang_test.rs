use keplr_lang::LangKind;
use std::path::Path;

#[test]
fn detects_tsx_cpp_go_rust_lm() {
    assert!(matches!(
        LangKind::from_path(Path::new("a.tsx")),
        LangKind::Tsx
    ));
    assert!(matches!(
        LangKind::from_path(Path::new("b.cpp")),
        LangKind::Cpp
    ));
    assert!(matches!(
        LangKind::from_path(Path::new("c.go")),
        LangKind::Go
    ));
    assert!(matches!(
        LangKind::from_path(Path::new("d.rs")),
        LangKind::Rust
    ));
    assert!(matches!(
        LangKind::from_path(Path::new("e.lm")),
        LangKind::Laml
    ));
    assert!(matches!(
        LangKind::from_path(Path::new("f.txt")),
        LangKind::Other
    ));
}
