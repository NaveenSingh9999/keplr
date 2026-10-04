//! Design-system drift guard.
//!
//! The rule this file exists to enforce: the browser client does not define a
//! single theme token. `keplr-theme` is the only place a colour, radius,
//! duration or border width is written down, `/theme.css` serves it, and
//! `ui.html` only ever reads `var(--k-*)`.
//!
//! The old guard compared a copy of the token block pasted into `ui.html`
//! against the crate, which meant every theme change had to be made twice and
//! the two copies drifted the moment anyone forgot. Comparing "does this file
//! define any token" to "does the crate serve them all" cannot drift.

use std::collections::BTreeMap;

fn ui_html() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/ui.html");
    std::fs::read_to_string(path).expect("ui.html reads")
}

/// Every custom property the theme crate serves, as `--name` -> value.
fn served_tokens() -> BTreeMap<String, String> {
    let css = keplr_theme::UserTheme::default().to_css();
    css.lines()
        .filter_map(|line| {
            let line = line.trim().strip_prefix("--k-")?;
            let (name, value) = line.split_once(':')?;
            Some((name.to_string(), value.trim_end_matches(';').to_string()))
        })
        .collect()
}

#[test]
fn ui_html_defines_no_theme_token() {
    let html = ui_html();
    for line in html.lines() {
        let trimmed = line.trim();
        // A definition is a custom property inside a declaration block. A use
        // is `var(--k-...)`, and a comment is a comment.
        if trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }
        assert!(
            !trimmed.starts_with("--k-"),
            "ui.html must not define {trimmed}: keplr-theme is the only place a token is written"
        );
    }
}

#[test]
fn every_served_token_has_a_name_the_theme_knows() {
    let tokens = served_tokens();
    for name in [
        "chrome",
        "surface",
        "raised",
        "overlay",
        "border",
        "border-strong",
        "text",
        "text-muted",
        "text-faint",
        "accent",
        "on-accent",
        "focus-ring",
        "selection",
        "current-line",
        "indent-guide",
        "scrollbar",
        "success",
        "warning",
        "error",
        "info",
        "syntax-keyword",
        "syntax-string",
        "syntax-number",
        "syntax-comment",
        "syntax-type",
        "syntax-function",
        "syntax-variable",
        "syntax-operator",
        "syntax-constant",
        "syntax-macro",
        "syntax-property",
        "font-size",
        "line-height",
        "radius-sm",
        "radius-md",
        "radius-lg",
        "border-width",
        "dur-fast",
        "dur-normal",
        "dur-slow",
        "ease",
        "spring",
        "reduced-motion",
    ] {
        assert!(tokens.contains_key(name), "the theme must serve --k-{name}");
    }
}

#[test]
fn the_css_is_a_root_block_of_custom_properties() {
    let css = keplr_theme::UserTheme::default().to_css();
    assert!(css.starts_with(":root{"), "it is one root block");
    assert!(css.trim_end().ends_with('}'), "and it closes");
    assert!(
        css.lines().all(|line| line.trim().is_empty()
            || line.starts_with(":root")
            || line.trim_start().starts_with("--k-")),
        "and it holds nothing but tokens"
    );
}

#[test]
fn the_browser_only_reads_tokens_the_theme_serves() {
    let html = ui_html();
    let tokens = served_tokens();
    // Every `var(--k-name` in the page must be a token the crate serves,
    // because a typo here is an invisible surface that silently falls back.
    let mut missing: Vec<String> = Vec::new();
    let bytes = html.as_bytes();
    let needle = b"var(--k-";
    let mut from = 0;
    while let Some(offset) = html[from..].find(std::str::from_utf8(needle).unwrap()) {
        let start = from + offset + needle.len();
        let rest = &html[start..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        if !tokens.contains_key(name) && !missing.iter().any(|m| m == name) {
            missing.push(name.to_string());
        }
        from = start + end.max(1);
    }
    assert!(
        missing.is_empty(),
        "the page reads tokens keplr-theme never serves: {missing:?}"
    );
    assert!(!bytes.is_empty());
}
