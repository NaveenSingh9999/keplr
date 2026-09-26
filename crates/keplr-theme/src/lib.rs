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
