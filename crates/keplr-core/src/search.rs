use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::path::PathBuf;

pub fn fuzzy_paths(paths: &[PathBuf], query: &str, limit: usize) -> Vec<PathBuf> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut scored: Vec<(u16, PathBuf)> = Vec::new();
    for p in paths {
        let s = p.to_string_lossy().to_string();
        let mut buf = Vec::new();
        let hay = Utf32Str::new(&s, &mut buf);
        let mut qbuf = Vec::new();
        let needle = Utf32Str::new(query, &mut qbuf);
        if let Some(score) = matcher.fuzzy_match(hay, needle) {
            scored.push((score, p.clone()));
        }
    }
    scored.sort_by_key(|a| std::cmp::Reverse(a.0));
    scored.into_iter().take(limit).map(|(_, p)| p).collect()
}
