//! Colours for the native window, taken from the user's theme.
//!
//! The field names match the theme's token names, so `chrome.text` and
//! `colors.text` cannot drift apart, and a new token is a new field here.

use keplr_theme::{parse_hex, UserTheme};
use rcus::Color;

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
    /// Parses every token, falling back to the value in `base` for anything the
    /// theme got wrong, so one bad colour cannot blank the window.
    pub fn new(theme: &UserTheme, base: &Chrome) -> Self {
        let colour = |token: &str, fallback: Color| match parse_hex(token) {
            Ok([r, g, b, a]) => Color::rgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ),
            Err(_) => fallback,
        };
        let c = &theme.colors;
        let s = &theme.syntax;
        Chrome {
            chrome: colour(&c.chrome, base.chrome),
            surface: colour(&c.surface, base.surface),
            raised: colour(&c.raised, base.raised),
            overlay: colour(&c.overlay, base.overlay),
            border: colour(&c.border, base.border),
            border_strong: colour(&c.border_strong, base.border_strong),
            text: colour(&c.text, base.text),
            text_muted: colour(&c.text_muted, base.text_muted),
            text_faint: colour(&c.text_faint, base.text_faint),
            accent: colour(&c.accent, base.accent),
            on_accent: colour(&c.on_accent, base.on_accent),
            focus_ring: colour(&c.focus_ring, base.focus_ring),
            selection: colour(&c.selection, base.selection),
            current_line: colour(&c.current_line, base.current_line),
            indent_guide: colour(&c.indent_guide, base.indent_guide),
            scrollbar: colour(&c.scrollbar, base.scrollbar),
            success: colour(&c.success, base.success),
            warning: colour(&c.warning, base.warning),
            error: colour(&c.error, base.error),
            info: colour(&c.info, base.info),
            syntax_keyword: colour(&s.keyword, base.syntax_keyword),
            syntax_string: colour(&s.string, base.syntax_string),
            syntax_number: colour(&s.number, base.syntax_number),
            syntax_comment: colour(&s.comment, base.syntax_comment),
            syntax_type: colour(&s.type_name, base.syntax_type),
            syntax_function: colour(&s.function, base.syntax_function),
            syntax_variable: colour(&s.variable, base.syntax_variable),
            syntax_operator: colour(&s.operator, base.syntax_operator),
            syntax_constant: colour(&s.constant, base.syntax_constant),
            syntax_macro: colour(&s.macro_name, base.syntax_macro),
            syntax_property: colour(&s.property, base.syntax_property),
            radius_sm: theme.ui.radius.sm,
            radius_md: theme.ui.radius.md,
            radius_lg: theme.ui.radius.lg,
            font_size: theme.ui.font_size,
            line_height: theme.ui.line_height,
            border_width: theme.ui.border_width,
        }
    }

    /// The window's own backdrop, which is the one colour nothing is drawn on.
    pub fn page(&self) -> Color {
        self.chrome
    }

    /// An all-zero palette, used only as the base a theme is merged onto when
    /// there is nothing to merge onto.
    #[cfg(test)]
    pub fn empty() -> Self {
        Chrome {
            chrome: Color::rgba(0.0, 0.0, 0.0, 1.0),
            surface: Color::rgba(0.0, 0.0, 0.0, 1.0),
            raised: Color::rgba(0.0, 0.0, 0.0, 1.0),
            overlay: Color::rgba(0.0, 0.0, 0.0, 1.0),
            border: Color::rgba(0.0, 0.0, 0.0, 1.0),
            border_strong: Color::rgba(0.0, 0.0, 0.0, 1.0),
            text: Color::rgba(1.0, 1.0, 1.0, 1.0),
            text_muted: Color::rgba(1.0, 1.0, 1.0, 1.0),
            text_faint: Color::rgba(1.0, 1.0, 1.0, 1.0),
            accent: Color::rgba(0.0, 0.0, 0.0, 1.0),
            on_accent: Color::rgba(0.0, 0.0, 0.0, 1.0),
            focus_ring: Color::rgba(0.0, 0.0, 0.0, 1.0),
            selection: Color::rgba(0.0, 0.0, 0.0, 1.0),
            current_line: Color::rgba(0.0, 0.0, 0.0, 1.0),
            indent_guide: Color::rgba(0.0, 0.0, 0.0, 1.0),
            scrollbar: Color::rgba(0.0, 0.0, 0.0, 1.0),
            success: Color::rgba(0.0, 0.0, 0.0, 1.0),
            warning: Color::rgba(0.0, 0.0, 0.0, 1.0),
            error: Color::rgba(0.0, 0.0, 0.0, 1.0),
            info: Color::rgba(0.0, 0.0, 0.0, 1.0),
            syntax_keyword: Color::rgba(1.0, 0.0, 1.0, 1.0),
            syntax_string: Color::rgba(1.0, 1.0, 0.0, 1.0),
            syntax_number: Color::rgba(0.0, 1.0, 0.0, 1.0),
            syntax_comment: Color::rgba(0.5, 0.5, 0.5, 1.0),
            syntax_type: Color::rgba(0.0, 1.0, 1.0, 1.0),
            syntax_function: Color::rgba(0.0, 0.0, 1.0, 1.0),
            syntax_variable: Color::rgba(1.0, 1.0, 1.0, 1.0),
            syntax_operator: Color::rgba(0.7, 0.7, 0.7, 1.0),
            syntax_constant: Color::rgba(1.0, 0.8, 0.0, 1.0),
            syntax_macro: Color::rgba(1.0, 0.0, 1.0, 1.0),
            syntax_property: Color::rgba(0.0, 1.0, 1.0, 1.0),
            radius_sm: 0.0,
            radius_md: 0.0,
            radius_lg: 0.0,
            font_size: 14.0,
            line_height: 1.2,
            border_width: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Chrome {
        Chrome::new(&UserTheme::default(), &Chrome::empty())
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
        let chrome = Chrome::new(&over, &base());
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
        let chrome = Chrome::new(&over, &base());
        assert_eq!(chrome.text, base().text, "one bad token costs one colour");
    }

    #[test]
    fn a_theme_changes_the_colours_a_view_would_draw_with() {
        let theme = UserTheme::default();
        let mut over = theme.clone();
        over.colors.accent = "#FF0000".to_string();
        let chrome = Chrome::new(&over, &base());
        assert!((chrome.accent.r - 1.0).abs() < 0.01);
        assert!((chrome.accent.g).abs() < 0.01);
    }

    #[test]
    fn radii_and_metrics_come_from_the_theme() {
        let theme = UserTheme::default();
        let mut over = theme.clone();
        over.ui.radius.md = 10.0;
        over.ui.font_size = 15.0;
        let chrome = Chrome::new(&over, &base());
        assert_eq!(chrome.radius_md, 10.0);
        assert_eq!(chrome.font_size, 15.0);
    }
}
