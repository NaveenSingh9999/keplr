//! Colours for the native window, taken from the user's theme.
//!
//! The field names match the theme's token names, so `chrome.text` and
//! `colors.text` cannot drift apart, and a new token is a new field here.

use keplr_theme::{parse_hex, UserTheme};
use rcus::Color;

/// Turns a parsed hex colour into the renderer's type.
fn to_colour([r, g, b, a]: [u8; 4]) -> Color {
    Color::rgba(
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    )
}

/// One colour per theme token, already parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct Chrome {
    pub chrome: Color,
    pub surface: Color,
    pub raised: Color,
    pub overlay: Color,
    pub border: Color,
    pub border_strong: Color,
    pub text: Color,
    pub text_muted: Color,
    pub text_faint: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub focus_ring: Color,
    pub selection: Color,
    pub current_line: Color,
    pub indent_guide: Color,
    pub scrollbar: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    /// Colours for code, by the token names the highlighter emits.
    pub syntax_keyword: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_comment: Color,
    pub syntax_type: Color,
    pub syntax_function: Color,
    pub syntax_variable: Color,
    pub syntax_operator: Color,
    pub syntax_constant: Color,
    pub syntax_macro: Color,
    pub syntax_property: Color,
    /// Corner radii, and the line height the editor draws with.
    pub radius_sm: f32,
    pub radius_md: f32,
    pub radius_lg: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub border_width: f32,
}

impl Chrome {
    /// A colour written in a cell or a terminal frame, parsed the same way a
    /// theme token is and falling back the same way.
    pub fn colour(&self, token: &str, fallback: Color) -> Color {
        match parse_hex(token) {
            Ok(rgba) => to_colour(rgba),
            Err(_) => fallback,
        }
    }

    /// Parses every token, falling back to the default theme's value for
    /// anything this theme got wrong, so one bad colour cannot blank the window.
    pub fn new(theme: &UserTheme) -> Self {
        let base = UserTheme::default();
        let colour = |token: &str, fallback: &str| match parse_hex(token) {
            Ok([r, g, b, a]) => Color::rgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ),
            Err(_) => parse_hex(fallback).map_or(Color::rgba(0.0, 0.0, 0.0, 1.0), to_colour),
        };
        let c = &theme.colors;
        let f = &base.colors;
        let s = &theme.syntax;
        let g = &base.syntax;
        Chrome {
            chrome: colour(&c.chrome, &f.chrome),
            surface: colour(&c.surface, &f.surface),
            raised: colour(&c.raised, &f.raised),
            overlay: colour(&c.overlay, &f.overlay),
            border: colour(&c.border, &f.border),
            border_strong: colour(&c.border_strong, &f.border_strong),
            text: colour(&c.text, &f.text),
            text_muted: colour(&c.text_muted, &f.text_muted),
            text_faint: colour(&c.text_faint, &f.text_faint),
            accent: colour(&c.accent, &f.accent),
            on_accent: colour(&c.on_accent, &f.on_accent),
            focus_ring: colour(&c.focus_ring, &f.focus_ring),
            selection: colour(&c.selection, &f.selection),
            current_line: colour(&c.current_line, &f.current_line),
            indent_guide: colour(&c.indent_guide, &f.indent_guide),
            scrollbar: colour(&c.scrollbar, &f.scrollbar),
            success: colour(&c.success, &f.success),
            warning: colour(&c.warning, &f.warning),
            error: colour(&c.error, &f.error),
            info: colour(&c.info, &f.info),
            syntax_keyword: colour(&s.keyword, &g.keyword),
            syntax_string: colour(&s.string, &g.string),
            syntax_number: colour(&s.number, &g.number),
            syntax_comment: colour(&s.comment, &g.comment),
            syntax_type: colour(&s.type_name, &g.type_name),
            syntax_function: colour(&s.function, &g.function),
            syntax_variable: colour(&s.variable, &g.variable),
            syntax_operator: colour(&s.operator, &g.operator),
            syntax_constant: colour(&s.constant, &g.constant),
            syntax_macro: colour(&s.macro_name, &g.macro_name),
            syntax_property: colour(&s.property, &g.property),
            radius_sm: if theme.ui.radius.sm > 0.0 {
                theme.ui.radius.sm
            } else {
                base.ui.radius.sm
            },
            radius_md: if theme.ui.radius.md > 0.0 {
                theme.ui.radius.md
            } else {
                base.ui.radius.md
            },
            radius_lg: if theme.ui.radius.lg > 0.0 {
                theme.ui.radius.lg
            } else {
                base.ui.radius.lg
            },
            font_size: if theme.ui.font_size > 0.0 {
                theme.ui.font_size
            } else {
                base.ui.font_size
            },
            line_height: if theme.ui.line_height > 0.0 {
                theme.ui.line_height
            } else {
                base.ui.line_height
            },
            border_width: if theme.ui.border_width > 0.0 {
                theme.ui.border_width
            } else {
                base.ui.border_width
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Chrome {
        Chrome::new(&UserTheme::default())
    }

    #[test]
    fn the_default_theme_gives_the_documented_text_colour() {
        let chrome = base();
        assert!(
            (chrome.text.r - 0.902).abs() < 0.01,
            "got {:?}",
            chrome.text
        );
        assert!(
            (chrome.accent.b - 1.0).abs() < 0.01,
            "got {:?}",
            chrome.accent
        );
    }

    #[test]
    fn an_eight_digit_token_carries_its_alpha() {
        let theme = UserTheme::default();
        let mut over = theme.clone();
        over.colors.chrome = "#10203080".to_string();
        let chrome = Chrome::new(&over);
        assert!(
            (chrome.chrome.a - 128.0 / 255.0).abs() < 0.01,
            "got {:?}",
            chrome.chrome
        );
    }

    #[test]
    fn a_bad_token_falls_back_instead_of_blanking_the_window() {
        let theme = UserTheme::default();
        let mut over = theme.clone();
        over.colors.text = "not a colour".to_string();
        let chrome = Chrome::new(&over);
        assert_eq!(chrome.text, base().text, "one bad token costs one colour");
    }

    #[test]
    fn a_theme_changes_the_colours_a_view_would_draw_with() {
        let theme = UserTheme::default();
        let mut over = theme.clone();
        over.colors.accent = "#FF0000".to_string();
        let chrome = Chrome::new(&over);
        assert!((chrome.accent.r - 1.0).abs() < 0.01);
        assert!((chrome.accent.g).abs() < 0.01);
    }

    #[test]
    fn radii_and_metrics_come_from_the_theme() {
        let theme = UserTheme::default();
        let mut over = theme.clone();
        over.ui.radius.md = 10.0;
        over.ui.font_size = 15.0;
        let chrome = Chrome::new(&over);
        assert_eq!(chrome.radius_md, 10.0);
        assert_eq!(chrome.font_size, 15.0);
    }

    #[test]
    fn a_metric_left_at_zero_is_inherited() {
        let theme = UserTheme::default();
        let mut over = theme.clone();
        over.ui.font_size = 0.0;
        over.ui.radius.md = 0.0;
        let chrome = Chrome::new(&over);
        assert_eq!(chrome.font_size, 13.0, "an unset metric keeps the default");
        assert_eq!(chrome.radius_md, 6.0);
    }
}
