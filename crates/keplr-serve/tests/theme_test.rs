//! Design-system drift guard: the `:root` block in ui.html must equal
//! `keplr-theme` output byte-for-byte. Edit tokens in the crate, never here.

fn root_block(html: &str) -> String {
    let start = html.find(":root {").expect("no :root block in ui.html");
    let end = html[start..]
        .find('}')
        .map(|i| start + i + 1)
        .expect("unclosed :root");
    html[start..end].to_string()
}

#[test]
fn ui_tokens_match_theme_crate() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui.html");
    let html = std::fs::read_to_string(path).unwrap();
    let expected = keplr_theme::Theme::amoled().to_css();
    // to_css() ends with "}\n"; the file block ends with "}".
    assert_eq!(root_block(&html), expected.trim_end().to_string());
}
