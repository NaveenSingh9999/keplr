//! Keplr design system — single source of truth for every surface.
//!
//! The web UI (`keplr-serve/src/ui.html`), the GPU snapshot renderer, the
//! WASM canvas and the TUI all derive their colors from [`Theme::amoled`].
//! A drift test in `keplr-serve` regenerates the `:root` CSS block from this
//! crate and fails the build if `ui.html` disagrees — edit tokens here.

use serde::{Deserialize, Serialize};

/// Full token set for the AMOLED ("Keplr Black") identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: &'static str,
    // surfaces
    pub bg: &'static str,
    pub raised: &'static str,
    pub pressed: &'static str,
    pub separator: &'static str,
    // text
    pub text: &'static str,
    pub text_secondary: &'static str,
    pub text_tertiary: &'static str,
    // accent + status
    pub accent: &'static str,
    pub ok: &'static str,
    pub warn: &'static str,
    pub error: &'static str,
    pub yellow: &'static str,
    pub purple: &'static str,
    pub teal: &'static str,
    // syntax (Xcode-dark-inspired, tuned for true black)
    pub syn_comment: &'static str,
    pub syn_keyword: &'static str,
    pub syn_string: &'static str,
    pub syn_number: &'static str,
    pub syn_type: &'static str,
    pub syn_function: &'static str,
    pub syn_macro: &'static str,
    pub syn_constant: &'static str,
    pub syn_plain: &'static str,
}

/// Apple system accent choices offered in Settings.
pub const ACCENTS: &[(&str, &str)] = &[
    ("blue", "#0A84FF"),
    ("green", "#30D158"),
    ("orange", "#FF9F0A"),
    ("red", "#FF453A"),
    ("purple", "#BF5AF2"),
    ("teal", "#64D2FF"),
    ("yellow", "#FFD60A"),
];

impl Theme {
    pub fn amoled() -> Self {
        Self {
            name: "amoled",
            bg: "#000000",
            raised: "#0D0D0F",
            pressed: "#1F1F22",
            separator: "rgba(255,255,255,.08)",
            text: "#F5F5F7",
            text_secondary: "#8D8D93",
            text_tertiary: "#48484A",
            accent: "#0A84FF",
            ok: "#30D158",
            warn: "#FF9F0A",
            error: "#FF453A",
            yellow: "#FFD60A",
            purple: "#BF5AF2",
            teal: "#64D2FF",
            syn_comment: "#6C7986",
            syn_keyword: "#FC5FA3",
            syn_string: "#FC6A5D",
            syn_number: "#D0BF69",
            syn_type: "#5DD8FF",
            syn_function: "#67B7A4",
            syn_macro: "#FD8F3F",
            syn_constant: "#A167E6",
            syn_plain: "#F5F5F7",
        }
    }

    /// `(css-variable, value)` pairs, in stable order.
    pub fn vars(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("--bg", self.bg),
            ("--surface", self.bg),
            ("--surface2", self.raised),
            ("--raised", self.raised),
            ("--pressed", self.pressed),
            ("--sep", self.separator),
            ("--border", self.separator),
            ("--text", self.text),
            ("--dim", self.text_secondary),
            ("--text2", self.text_secondary),
            ("--text3", self.text_tertiary),
            ("--accent", self.accent),
            ("--ok", self.ok),
            ("--warn", self.warn),
            ("--error", self.error),
            ("--yellow", self.yellow),
            ("--purple", self.purple),
            ("--teal", self.teal),
            ("--syn-comment", self.syn_comment),
            ("--syn-keyword", self.syn_keyword),
            ("--syn-string", self.syn_string),
            ("--syn-number", self.syn_number),
            ("--syn-type", self.syn_type),
            ("--syn-function", self.syn_function),
            ("--syn-macro", self.syn_macro),
            ("--syn-constant", self.syn_constant),
            ("--syn-plain", self.syn_plain),
        ]
    }

    /// Render the `:root { … }` CSS block byte-identically every time.
    /// Non-color tokens (fonts, radii, motion) live in `static_tokens()`
    /// because they are platform CSS, not portable color values.
    pub fn to_css(&self) -> String {
        let mut s = String::from(":root {\n");
        for (k, v) in self.vars() {
            s.push_str(&format!("  {k}: {v};\n"));
        }
        s.push_str(static_tokens());
        s.push_str("}\n");
        s
    }
}

/// Font, geometry and motion tokens: identical for every theme.
pub fn static_tokens() -> &'static str {
    r#"  --overlay: rgba(22,22,24,.85);
  --overlay-blur: 24px;
  --on-accent: #ffffff;
  --accent-soft: color-mix(in srgb, var(--accent) 16%, transparent);
  --font-ui: -apple-system, "SF Pro Text", Inter, system-ui, "Segoe UI", Roboto, sans-serif;
  --font-mono: "SF Mono", "JetBrains Mono", ui-monospace, Menlo, Consolas, monospace;
  --fs-cap: 11px;
  --fs-ui: 13px;
  --fs-code: 13px;
  --lh-code: 1.6;
  --r-sm: 6px;
  --r-md: 10px;
  --r-lg: 14px;
  --h-ctl: 28px;
  --ease: cubic-bezier(.32,.72,0,1);
  --d1: 150ms;
  --d2: 280ms;
"#
}

pub mod discovery;
pub mod theme;

pub use discovery::{discover, load, problems, resolve, themes_dir, DiscoveredTheme, ThemeSource};
pub use theme::{
    parse_hex, Appearance, ColorTokens, Density, MotionTokens, Radius, SpringTokens, SyntaxTokens,
    UiTokens, UserTheme,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_is_deterministic() {
        assert_eq!(Theme::amoled().to_css(), Theme::amoled().to_css());
    }

    #[test]
    fn secondary_text_contrast() {
        // #8D8D93 on #000000 ≈ 7:1 — comfortably above WCAG AA 4.5:1.
        // (Luminance math inlined to keep the crate dependency-free.)
        fn lum(hex: &str) -> f64 {
            let c = |i: usize| {
                let v = u8::from_str_radix(&hex[i..i + 2], 16).unwrap() as f64 / 255.0;
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * c(1) + 0.7152 * c(3) + 0.0722 * c(5)
        }
        let (l1, l2) = (lum("#8D8D93"), lum("#000000"));
        let ratio = (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05);
        assert!(ratio >= 4.5, "contrast {ratio}");
    }
}

#[cfg(test)]
mod user_theme_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_default_theme_is_the_one_the_spec_describes() {
        let theme = UserTheme::default();
        assert_eq!(theme.colors.accent, "#0A84FF");
        assert_eq!(theme.syntax.keyword, "#BF5AF2");
        assert_eq!(theme.ui.font_size, 13.0);
        assert_eq!(theme.ui.radius.md, 6.0);
        assert_eq!(theme.motion.normal_ms, 180);
        assert!(!theme.motion.reduced_motion);
    }

    #[test]
    fn an_empty_object_decodes_to_the_default_theme() {
        let over: UserTheme = serde_json::from_str("{}").expect("decodes");
        assert_eq!(UserTheme::merged(&over), UserTheme::default());
    }

    #[test]
    fn one_colour_overrides_only_that_colour() {
        let over: UserTheme =
            serde_json::from_str(r##"{"colors":{"accent":"#FF0000"}}"##).expect("decodes");
        let merged = UserTheme::merged(&over);
        assert_eq!(merged.colors.accent, "#FF0000");
        assert_eq!(merged.colors.text, "#E6E6EA", "the rest is untouched");
    }

    #[test]
    fn a_theme_may_name_itself_and_be_dark_or_light() {
        let over: UserTheme =
            serde_json::from_str(r#"{"name":"Solar","appearance":"light"}"#).expect("decodes");
        let merged = UserTheme::merged(&over);
        assert_eq!(merged.name, "Solar");
        assert_eq!(merged.appearance, Appearance::Light);
    }

    #[test]
    fn a_duration_longer_than_the_ceiling_is_clamped() {
        let over: UserTheme =
            serde_json::from_str(r#"{"motion":{"fastMs":9999}}"#).expect("decodes");
        assert_eq!(UserTheme::merged(&over).motion.fast_ms, 400);
    }

    #[test]
    fn a_bad_colour_is_named_rather_than_ignored() {
        let over: UserTheme =
            serde_json::from_str(r##"{"colors":{"accent":"chartreuse"}}"##).expect("decodes");
        let problems = UserTheme::merged(&over).problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("colors.accent"), "{}", problems[0]);
    }

    #[test]
    fn a_bad_font_size_is_reported_too() {
        let over: UserTheme = serde_json::from_str(r#"{"ui":{"fontSize":400}}"#).expect("decodes");
        let problems = UserTheme::merged(&over).problems();
        assert!(
            problems.iter().any(|p| p.starts_with("ui.fontSize")),
            "{problems:?}"
        );
    }

    #[test]
    fn a_good_theme_has_nothing_to_report() {
        assert!(UserTheme::default().problems().is_empty());
    }

    #[test]
    fn hex_accepts_three_six_and_eight_digits() {
        assert_eq!(theme::parse_hex("#fff").unwrap(), [255, 255, 255, 255]);
        assert_eq!(theme::parse_hex("#102030").unwrap(), [16, 32, 48, 255]);
        assert_eq!(theme::parse_hex("#10203080").unwrap(), [16, 32, 48, 128]);
        assert!(
            theme::parse_hex("102030").is_err(),
            "a missing # is not a colour"
        );
        assert!(
            theme::parse_hex("#12345").is_err(),
            "five digits is not a colour"
        );
    }

    #[test]
    fn the_css_block_carries_every_token() {
        let css = UserTheme::default().to_css();
        for name in [
            "--k-chrome:#000000",
            "--k-accent:#0A84FF",
            "--k-syntax-keyword:#BF5AF2",
            "--k-syntax-macro:#BF5AF2",
            "--k-radius-md:6px",
            "--k-dur-normal:180ms",
            "--k-ease:cubic-bezier(0.2,0,0,1)",
            "--k-reduced-motion:0",
        ] {
            assert!(css.contains(name), "missing {name} in\n{css}");
        }
        assert!(css.starts_with(":root{") && css.trim_end().ends_with('}'));
    }

    #[test]
    fn the_css_block_is_stable() {
        assert_eq!(UserTheme::default().to_css(), UserTheme::default().to_css());
    }

    #[test]
    fn a_workspace_without_a_theme_directory_still_has_the_built_in_one() {
        let found = discovery::discover(Path::new("/tmp/keplr-no-such-workspace"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, ThemeSource::BuiltIn);
        assert_eq!(found[0].theme, UserTheme::default());
    }

    #[test]
    fn a_theme_file_is_found_and_layered_over_the_default() {
        let root = std::env::temp_dir().join("keplr-theme-test-workspace");
        let dir = root.join(".keplr").join("themes");
        std::fs::create_dir_all(&dir).expect("fixture");
        std::fs::write(
            dir.join("mine.json"),
            r##"{"name":"Mine","colors":{"accent":"#00FF00"}}"##,
        )
        .expect("fixture");
        let found = discovery::discover(&root);
        assert_eq!(found.len(), 2, "the built-in plus the user's");
        let mine = found
            .iter()
            .find(|t| t.name == "Mine")
            .expect("found by name");
        assert_eq!(mine.source, ThemeSource::File);
        assert_eq!(mine.theme.colors.accent, "#00FF00");
        assert!(
            discovery::problems(&root).is_empty(),
            "{:?}",
            discovery::problems(&root)
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_broken_theme_file_is_reported_and_skipped() {
        let root = std::env::temp_dir().join("keplr-theme-broken-workspace");
        let dir = root.join(".keplr").join("themes");
        std::fs::create_dir_all(&dir).expect("fixture");
        std::fs::write(dir.join("broken.json"), "{not json").expect("fixture");
        std::fs::write(dir.join("huge.json"), vec![b'x'; 300_000]).expect("fixture");
        let found = discovery::discover(&root);
        assert_eq!(found.len(), 1, "only the built-in survives");
        let problems = discovery::problems(&root);
        assert!(
            problems.iter().any(|p| p.contains("broken.json")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|p| p.contains("huge.json")),
            "{problems:?}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_named_theme_is_chosen_and_an_unknown_name_falls_back() {
        let root = std::env::temp_dir().join("keplr-theme-resolve-workspace");
        let dir = root.join(".keplr").join("themes");
        std::fs::create_dir_all(&dir).expect("fixture");
        std::fs::write(dir.join("a.json"), r#"{"name":"Alpha"}"#).expect("fixture");
        assert_eq!(discovery::resolve(&root, Some("Alpha")).name, "Alpha");
        assert_eq!(discovery::resolve(&root, Some("nope")).name, "Keplr Dark");
        assert_eq!(
            discovery::resolve(&root, None).name,
            "Alpha",
            "the first by name"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn every_colour_token_is_listed_for_a_picker() {
        let theme = UserTheme::default();
        let table = theme::colour_table(&theme);
        assert_eq!(table.len(), 20);
        assert_eq!(table.get("accent"), Some(&"#0A84FF"));
    }
}
