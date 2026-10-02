//! A theme a user wrote, as data.
//!
//! One schema, two renderers. The native client reads these values straight
//! into its styles; the browser client turns the same file into CSS custom
//! properties. There is no privileged theme and no second format, so a file
//! dropped into `.keplr/themes/` looks identical in both.
//!
//! Every token defaults, so a theme may set one colour and inherit the rest.
//! Anything malformed is reported by [`UserTheme::problems`] rather than
//! replacing the window with nothing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A colour as a CSS token, kept as the string the user wrote so a theme file
/// round-trips and a mistake can be pointed at.
pub type Hex = String;

/// Whether a theme is meant for a dark or a light window, which decides which
/// way shadows and hairlines are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    #[default]
    Dark,
    Light,
    HighContrast,
}

/// How much room everything gets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiTokens {
    pub font_size: f32,
    pub line_height: f32,
    /// `compact` tightens every bar and gutter by a few pixels.
    pub density: Density,
    pub border_width: f32,
    pub radius: Radius,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

impl Default for Radius {
    /// Zero everywhere: a box with no radius is a rectangle, which is the safe
    /// default for a theme that never mentions one.
    fn default() -> Self {
        Self {
            sm: 0.0,
            md: 0.0,
            lg: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Radius {
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
}

/// Every surface, border and text colour the shell draws.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ColorTokens {
    pub chrome: Hex,
    pub surface: Hex,
    pub raised: Hex,
    pub overlay: Hex,
    pub border: Hex,
    pub border_strong: Hex,
    pub text: Hex,
    pub text_muted: Hex,
    pub text_faint: Hex,
    pub accent: Hex,
    pub on_accent: Hex,
    pub focus_ring: Hex,
    pub selection: Hex,
    pub current_line: Hex,
    pub indent_guide: Hex,
    pub scrollbar: Hex,
    pub success: Hex,
    pub warning: Hex,
    pub error: Hex,
    pub info: Hex,
}

/// Colours for code, by the token names the highlighter already emits.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SyntaxTokens {
    pub keyword: Hex,
    pub string: Hex,
    pub number: Hex,
    pub comment: Hex,
    pub type_name: Hex,
    pub function: Hex,
    pub variable: Hex,
    pub operator: Hex,
    pub constant: Hex,
    pub macro_name: Hex,
    pub property: Hex,
}

/// How long things take and how they move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MotionTokens {
    pub fast_ms: u16,
    pub normal_ms: u16,
    pub slow_ms: u16,
    /// `cubic-bezier(x1, y1, x2, y2)`.
    pub ease: [f32; 4],
    pub spring: SpringTokens,
    /// Collapses every duration to zero while keeping the same end states.
    pub reduced_motion: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SpringTokens {
    pub stiffness: f32,
    pub damping: f32,
}

/// A whole theme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UserTheme {
    pub name: String,
    pub appearance: Appearance,
    pub ui: UiTokens,
    pub colors: ColorTokens,
    pub syntax: SyntaxTokens,
    pub motion: MotionTokens,
}

impl Default for UserTheme {
    fn default() -> Self {
        Self {
            name: "Keplr Dark".to_string(),
            appearance: Appearance::Dark,
            ui: UiTokens {
                font_size: 13.0,
                line_height: 1.5,
                density: Density::Comfortable,
                border_width: 1.0,
                radius: Radius {
                    sm: 4.0,
                    md: 6.0,
                    lg: 10.0,
                },
            },
            colors: ColorTokens {
                chrome: "#000000".into(),
                surface: "#0D0D0F".into(),
                raised: "#141417".into(),
                overlay: "#1C1C20".into(),
                border: "#232327".into(),
                border_strong: "#2E2E34".into(),
                text: "#E6E6EA".into(),
                text_muted: "#8A8A93".into(),
                text_faint: "#5A5A63".into(),
                accent: "#0A84FF".into(),
                on_accent: "#FFFFFF".into(),
                focus_ring: "#0A84FF".into(),
                selection: "#264F78".into(),
                current_line: "#FFFFFF0A".into(),
                indent_guide: "#FFFFFF12".into(),
                scrollbar: "#FFFFFF1F".into(),
                success: "#30D158".into(),
                warning: "#FFD60A".into(),
                error: "#FF453A".into(),
                info: "#64D2FF".into(),
            },
            syntax: SyntaxTokens {
                keyword: "#BF5AF2".into(),
                string: "#FF9F0A".into(),
                number: "#30D158".into(),
                comment: "#5A5A63".into(),
                type_name: "#64D2FF".into(),
                function: "#0A84FF".into(),
                variable: "#E6E6EA".into(),
                operator: "#8A8A93".into(),
                constant: "#FF9F0A".into(),
                macro_name: "#BF5AF2".into(),
                property: "#64D2FF".into(),
            },
            motion: MotionTokens {
                fast_ms: 120,
                normal_ms: 180,
                slow_ms: 260,
                ease: [0.2, 0.0, 0.0, 1.0],
                spring: SpringTokens {
                    stiffness: 220.0,
                    damping: 26.0,
                },
                reduced_motion: false,
            },
        }
    }
}

/// The longest a duration may be. A theme asking for ten seconds is a mistake,
/// not a design.
const MAX_DURATION_MS: u16 = 400;

impl UserTheme {
    /// A theme layered over the default: every key the user set wins, every key
    /// they left out stays as it was.
    pub fn merged(over: &UserTheme) -> UserTheme {
        let base = UserTheme::default();
        UserTheme {
            name: if over.name.trim().is_empty() {
                base.name.clone()
            } else {
                over.name.clone()
            },
            appearance: over.appearance,
            ui: merge_ui(&base.ui, &over.ui),
            colors: merge_colors(&base.colors, &over.colors),
            syntax: merge_syntax(&base.syntax, &over.syntax),
            motion: merge_motion(&base.motion, &over.motion),
        }
    }

    /// Everything wrong with this theme, phrased for a status bar. An empty list
    /// means it is usable.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let colours: [(&str, &str); 20] = [
            ("chrome", &self.colors.chrome),
            ("surface", &self.colors.surface),
            ("raised", &self.colors.raised),
            ("overlay", &self.colors.overlay),
            ("border", &self.colors.border),
            ("borderStrong", &self.colors.border_strong),
            ("text", &self.colors.text),
            ("textMuted", &self.colors.text_muted),
            ("textFaint", &self.colors.text_faint),
            ("accent", &self.colors.accent),
            ("onAccent", &self.colors.on_accent),
            ("focusRing", &self.colors.focus_ring),
            ("selection", &self.colors.selection),
            ("currentLine", &self.colors.current_line),
            ("indentGuide", &self.colors.indent_guide),
            ("scrollbar", &self.colors.scrollbar),
            ("success", &self.colors.success),
            ("warning", &self.colors.warning),
            ("error", &self.colors.error),
            ("info", &self.colors.info),
        ];
        for (key, value) in colours {
            if let Err(problem) = parse_hex(value) {
                out.push(format!("colors.{key} {problem}"));
            }
        }
        let syntax: [(&str, &str); 11] = [
            ("keyword", &self.syntax.keyword),
            ("string", &self.syntax.string),
            ("number", &self.syntax.number),
            ("comment", &self.syntax.comment),
            ("type", &self.syntax.type_name),
            ("function", &self.syntax.function),
            ("variable", &self.syntax.variable),
            ("operator", &self.syntax.operator),
            ("constant", &self.syntax.constant),
            ("macro", &self.syntax.macro_name),
            ("property", &self.syntax.property),
        ];
        for (key, value) in syntax {
            if let Err(problem) = parse_hex(value) {
                out.push(format!("syntax.{key} {problem}"));
            }
        }
        if self.ui.font_size < 6.0 || self.ui.font_size > 96.0 {
            out.push(format!(
                "ui.fontSize {} is not between 6 and 96",
                self.ui.font_size
            ));
        }
        if self.ui.line_height < 0.8 || self.ui.line_height > 3.0 {
            out.push(format!(
                "ui.lineHeight {} is not between 0.8 and 3",
                self.ui.line_height
            ));
        }
        for (key, value) in [
            ("fastMs", self.motion.fast_ms),
            ("normalMs", self.motion.normal_ms),
            ("slowMs", self.motion.slow_ms),
        ] {
            if value > MAX_DURATION_MS {
                out.push(format!(
                    "motion.{key} {value}ms is longer than {MAX_DURATION_MS}ms"
                ));
            }
        }
        out
    }

    /// The `:root` block the browser client uses, generated from these values so
    /// the two clients cannot disagree about a colour.
    pub fn to_css(&self) -> String {
        let mut css = String::from(":root{\n");
        let mut put = |name: &str, value: &str| {
            css.push_str(&format!("  --k-{name}:{value};\n"));
        };
        for (name, value) in [
            ("chrome", &self.colors.chrome),
            ("surface", &self.colors.surface),
            ("raised", &self.colors.raised),
            ("overlay", &self.colors.overlay),
            ("border", &self.colors.border),
            ("border-strong", &self.colors.border_strong),
            ("text", &self.colors.text),
            ("text-muted", &self.colors.text_muted),
            ("text-faint", &self.colors.text_faint),
            ("accent", &self.colors.accent),
            ("on-accent", &self.colors.on_accent),
            ("focus-ring", &self.colors.focus_ring),
            ("selection", &self.colors.selection),
            ("current-line", &self.colors.current_line),
            ("indent-guide", &self.colors.indent_guide),
            ("scrollbar", &self.colors.scrollbar),
            ("success", &self.colors.success),
            ("warning", &self.colors.warning),
            ("error", &self.colors.error),
            ("info", &self.colors.info),
        ] {
            put(name, value);
        }
        for (name, value) in [
            ("syntax-keyword", &self.syntax.keyword),
            ("syntax-string", &self.syntax.string),
            ("syntax-number", &self.syntax.number),
            ("syntax-comment", &self.syntax.comment),
            ("syntax-type", &self.syntax.type_name),
            ("syntax-function", &self.syntax.function),
            ("syntax-variable", &self.syntax.variable),
            ("syntax-operator", &self.syntax.operator),
            ("syntax-constant", &self.syntax.constant),
            ("syntax-macro", &self.syntax.macro_name),
            ("syntax-property", &self.syntax.property),
        ] {
            put(name, value);
        }
        put("font-size", &format!("{}px", self.ui.font_size));
        put("line-height", &format!("{}", self.ui.line_height));
        put("radius-sm", &format!("{}px", self.ui.radius.sm));
        put("radius-md", &format!("{}px", self.ui.radius.md));
        put("radius-lg", &format!("{}px", self.ui.radius.lg));
        put("border-width", &format!("{}px", self.ui.border_width));
        put("dur-fast", &format!("{}ms", self.motion.fast_ms));
        put("dur-normal", &format!("{}ms", self.motion.normal_ms));
        put("dur-slow", &format!("{}ms", self.motion.slow_ms));
        put(
            "ease",
            &format!(
                "cubic-bezier({},{},{},{})",
                self.motion.ease[0], self.motion.ease[1], self.motion.ease[2], self.motion.ease[3]
            ),
        );
        put(
            "spring",
            &format!(
                "{} {}",
                if self.motion.reduced_motion {
                    0.0
                } else {
                    self.motion.spring.stiffness
                },
                if self.motion.reduced_motion {
                    1.0
                } else {
                    self.motion.spring.damping
                }
            ),
        );
        put(
            "reduced-motion",
            if self.motion.reduced_motion { "1" } else { "0" },
        );
        css.push_str("}\n");
        css
    }
}

/// Every colour token as a name/value pair, for a picker or a diff.
pub fn colour_table(theme: &UserTheme) -> BTreeMap<&'static str, &str> {
    BTreeMap::from([
        ("chrome", theme.colors.chrome.as_str()),
        ("surface", theme.colors.surface.as_str()),
        ("raised", theme.colors.raised.as_str()),
        ("overlay", theme.colors.overlay.as_str()),
        ("border", theme.colors.border.as_str()),
        ("borderStrong", theme.colors.border_strong.as_str()),
        ("text", theme.colors.text.as_str()),
        ("textMuted", theme.colors.text_muted.as_str()),
        ("textFaint", theme.colors.text_faint.as_str()),
        ("accent", theme.colors.accent.as_str()),
        ("onAccent", theme.colors.on_accent.as_str()),
        ("focusRing", theme.colors.focus_ring.as_str()),
        ("selection", theme.colors.selection.as_str()),
        ("currentLine", theme.colors.current_line.as_str()),
        ("indentGuide", theme.colors.indent_guide.as_str()),
        ("scrollbar", theme.colors.scrollbar.as_str()),
        ("success", theme.colors.success.as_str()),
        ("warning", theme.colors.warning.as_str()),
        ("error", theme.colors.error.as_str()),
        ("info", theme.colors.info.as_str()),
    ])
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`, and nothing else.
pub fn parse_hex(token: &str) -> Result<[u8; 4], String> {
    let Some(hex) = token.trim().strip_prefix('#') else {
        return Err(format!("{token:?} is not a #hex colour"));
    };
    let expand =
        |value: &str| u8::from_str_radix(value, 16).map_err(|_| format!("{token:?} is not hex"));
    match hex.len() {
        3 => {
            let d: Vec<u8> = hex
                .chars()
                .map(|c| u8::from_str_radix(&format!("{c}{c}"), 16))
                .collect::<Result<_, _>>()
                .map_err(|_| format!("{token:?} is not hex"))?;
            Ok([d[0], d[1], d[2], 255])
        }
        6 => Ok([
            expand(&hex[0..2]).map_err(|e| e)?,
            expand(&hex[2..4]).map_err(|e| e)?,
            expand(&hex[4..6]).map_err(|e| e)?,
            255,
        ]),
        8 => Ok([
            expand(&hex[0..2]).map_err(|e| e)?,
            expand(&hex[2..4]).map_err(|e| e)?,
            expand(&hex[4..6]).map_err(|e| e)?,
            expand(&hex[6..8]).map_err(|e| e)?,
        ]),
        other => Err(format!("{token:?} has {other} digits, expected 3, 6 or 8")),
    }
}

/// A token is "unset" when it holds its type's zero value, which for a colour
/// is an empty string: the user simply did not mention it.
trait Unset {
    fn is_unset(&self) -> bool;
}

impl Unset for String {
    fn is_unset(&self) -> bool {
        self.is_empty()
    }
}

macro_rules! merge_fields {
    ($base:expr, $over:expr, $($field:ident),+ $(,)?) => {{
        let mut out = $base.clone();
        $( if !$over.$field.is_unset() { out.$field = $over.$field.clone(); } )+
        out
    }};
}

fn merge_ui(base: &UiTokens, over: &UiTokens) -> UiTokens {
    UiTokens {
        font_size: if over.font_size != 0.0 {
            over.font_size
        } else {
            base.font_size
        },
        line_height: if over.line_height != 0.0 {
            over.line_height
        } else {
            base.line_height
        },
        density: over.density,
        border_width: if over.border_width != 0.0 {
            over.border_width
        } else {
            base.border_width
        },
        radius: if over.radius != Radius::default() {
            over.radius
        } else {
            base.radius
        },
    }
}

fn merge_colors(base: &ColorTokens, over: &ColorTokens) -> ColorTokens {
    merge_fields!(
        base,
        over,
        chrome,
        surface,
        raised,
        overlay,
        border,
        border_strong,
        text,
        text_muted,
        text_faint,
        accent,
        on_accent,
        focus_ring,
        selection,
        current_line,
        indent_guide,
        scrollbar,
        success,
        warning,
        error,
        info
    )
}

fn merge_syntax(base: &SyntaxTokens, over: &SyntaxTokens) -> SyntaxTokens {
    merge_fields!(
        base, over, keyword, string, number, comment, type_name, function, variable, operator,
        constant, macro_name, property
    )
}

fn merge_motion(base: &MotionTokens, over: &MotionTokens) -> MotionTokens {
    MotionTokens {
        fast_ms: over.fast_ms.min(MAX_DURATION_MS),
        normal_ms: over.normal_ms.min(MAX_DURATION_MS),
        slow_ms: over.slow_ms.min(MAX_DURATION_MS),
        // A zero duration is what reduced motion asks for, so it is kept rather
        // than treated as "unset".
        ease: over.ease,
        spring: over.spring,
        reduced_motion: over.reduced_motion || base.reduced_motion,
    }
}
