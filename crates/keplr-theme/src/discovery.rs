//! Finding the themes a workspace has.
//!
//! A theme is a JSON file in `<root>/.keplr/themes/`. There is nothing to
//! install and nothing to enable: the file is the theme, and dropping it in is
//! the whole workflow. Anything unreadable is skipped and reported rather than
//! allowed to blank a window.

use std::path::{Path, PathBuf};

use crate::theme::UserTheme;

/// Where a theme came from.
///
/// Ordered so that a workspace's own themes come first: someone who dropped a
/// file in wants that file, and the built-in is the fallback rather than the
/// default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThemeSource {
    /// Written in this workspace.
    File,
    /// Shipped with Keplr.
    BuiltIn,
}

/// One theme the window can switch to.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredTheme {
    pub name: String,
    pub path: PathBuf,
    pub source: ThemeSource,
    /// The theme with the user's values layered over the defaults.
    pub theme: UserTheme,
}

/// The directory a workspace keeps its themes in.
pub fn themes_dir(root: &Path) -> PathBuf {
    root.join(".keplr").join("themes")
}

/// A theme file larger than this is not a theme.
const MAX_THEME_BYTES: u64 = 256 * 1024;

/// Every usable theme under `root`, sorted by name, with the built-in first.
///
/// A file that cannot be read, parsed or understood is left out and named by
/// [`problems`], so a broken theme costs one entry rather than the whole list.
pub fn discover(root: &Path) -> Vec<DiscoveredTheme> {
    let mut found = vec![DiscoveredTheme {
        name: "Keplr Dark".to_string(),
        path: PathBuf::from("<built-in>"),
        source: ThemeSource::BuiltIn,
        theme: UserTheme::default(),
    }];
    let dir = themes_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return found;
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    for path in files {
        if let Some(theme) = load(&path) {
            found.push(theme);
        }
    }
    found.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    found
}

/// Reads one theme file, returning `None` if it cannot be used.
pub fn load(path: &Path) -> Option<DiscoveredTheme> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_THEME_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let over: UserTheme = serde_json::from_value(parsed).ok()?;
    let theme = UserTheme::merged(&over);
    let name = theme.name.clone();
    Some(DiscoveredTheme {
        name,
        path: path.to_path_buf(),
        source: ThemeSource::File,
        theme,
    })
}

/// Everything wrong with the themes under `root`, phrased for a status bar.
pub fn problems(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let dir = themes_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "json") {
            continue;
        }
        let label = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let meta = std::fs::metadata(&path).ok();
        if meta.is_some_and(|meta| meta.len() > MAX_THEME_BYTES) {
            out.push(format!(
                "{label} is larger than 256 KB, so it is not a theme"
            ));
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            out.push(format!("{label} could not be read"));
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            out.push(format!("{label} is not valid JSON"));
            continue;
        };
        let Ok(over) = serde_json::from_value::<UserTheme>(value) else {
            out.push(format!("{label} does not match the theme format"));
            continue;
        };
        // Check what will actually be drawn: a file that sets one colour is
        // meant to inherit the rest, not to be reported as twenty empty ones.
        for problem in UserTheme::merged(&over).problems() {
            out.push(format!("{label}: {problem}"));
        }
    }
    out
}

/// The theme a window should open with: the one named by `KEPLR_THEME`, or the
/// first theme the workspace has.
pub fn resolve(root: &Path, wanted: Option<&str>) -> UserTheme {
    let themes = discover(root);
    match wanted {
        Some(name) => themes
            .iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
            .map(|theme| theme.theme.clone())
            // A name that is not here falls back to the built-in rather than to
            // whichever workspace theme happens to sort first, so the window is
            // the same whatever was asked for.
            .unwrap_or_default(),
        None => themes
            .first()
            .map(|theme| theme.theme.clone())
            .unwrap_or_default(),
    }
}
