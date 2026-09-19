use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct ScreenGuard;

impl ScreenGuard {
    fn enter() -> anyhow::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, Hide)?;
        Ok(Self)
    }
}

impl Drop for ScreenGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
    }
}

struct Doc {
    lines: Vec<String>,
    ends_nl: bool,
}

impl Doc {
    fn load(full: &Path) -> Self {
        let text = std::fs::read_to_string(full).unwrap_or_default();
        let ends_nl = text.ends_with('\n');
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self { lines, ends_nl }
    }

    fn content(&self) -> String {
        let mut s = self.lines.join("\n");
        if self.ends_nl {
            s.push('\n');
        }
        s
    }

    fn byte_col(&self, line: usize, col: usize) -> usize {
        self.lines
            .get(line)
            .and_then(|l| l.char_indices().nth(col).map(|(i, _)| i))
            .unwrap_or_else(|| self.lines.get(line).map(|l| l.len()).unwrap_or(0))
    }

    fn line_chars(&self, line: usize) -> usize {
        self.lines.get(line).map(|l| l.chars().count()).unwrap_or(0)
    }
}

struct TabState {
    path: PathBuf,
    label: String,
    lang: keplr_lang::LangKind,
    doc: Doc,
    cursor: (usize, usize),
    top: usize,
    top_f: f32,
    dirty: bool,
    diags: Vec<keplr_lang::Diagnostic>,
}

fn open_tab(root: &Path, rel_or_abs: &Path) -> TabState {
    let full = if rel_or_abs.is_absolute() {
        rel_or_abs.to_path_buf()
    } else {
        root.join(rel_or_abs)
    };
    let label = full
        .strip_prefix(root)
        .unwrap_or(&full)
        .display()
        .to_string();
    let lang = keplr_lang::LangKind::from_path(&full);
    let doc = Doc::load(&full);
    let diags = if lang == keplr_lang::LangKind::Laml {
        keplr_lang::laml_diagnostics(&full)
    } else {
        Vec::new()
    };
    TabState {
        path: full,
        label,
        lang,
        doc,
        cursor: (0, 0),
        top: 0,
        top_f: 0.0,
        dirty: false,
        diags,
    }
}

fn refresh_diags(tab: &mut TabState) {
    tab.diags = if tab.lang == keplr_lang::LangKind::Laml {
        keplr_lang::laml_diagnostics(&tab.path)
    } else {
        Vec::new()
    };
}

fn byte_slice(text: &str, start: usize, len: usize) -> &str {
    let end = (start + len).min(text.len());
    let mut s = start.min(text.len());
    while s < text.len() && !text.is_char_boundary(s) {
        s += 1;
    }
    let mut e = end;
    while e > s && !text.is_char_boundary(e) {
        e -= 1;
    }
    &text[s..e]
}

fn colored_spans(lang: keplr_lang::LangKind, line: &str, max: usize) -> String {
    let text: String = line.chars().take(max).collect();
    let spans = keplr_lang::highlight(lang, &text);
    if spans.iter().all(|s| s.kind == keplr_lang::TokenKind::Other) {
        return text;
    }
    let mut out = String::new();
    for s in &spans {
        let piece = byte_slice(&text, s.start, s.len);
        match s.kind {
            keplr_lang::TokenKind::Keyword => {
                out.push_str(&format!("\x1b[1;36m{piece}\x1b[0m"));
            }
            keplr_lang::TokenKind::Str => {
                out.push_str(&format!("\x1b[32m{piece}\x1b[0m"));
            }
            keplr_lang::TokenKind::Comment => {
                out.push_str(&format!("\x1b[2m{piece}\x1b[0m"));
            }
            keplr_lang::TokenKind::Number => {
                out.push_str(&format!("\x1b[33m{piece}\x1b[0m"));
            }
            keplr_lang::TokenKind::Other => {
                out.push_str(piece);
            }
        }
    }
    out
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn reduced_motion(no_flag: bool) -> bool {
    no_flag
        || std::env::var("KEPLR_REDUCED_MOTION")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
}

struct Frame<'a> {
    doc: &'a Doc,
    file_label: &'a str,
    branch: &'a str,
    lang: keplr_lang::LangKind,
    cursor: (usize, usize),
    cursor_visible: bool,
    top_render: usize,
    dirty: bool,
    status: &'a str,
    tabs: &'a [(String, bool, bool)],
    palette_open: bool,
    palette_query: &'a str,
    palette_hits: &'a [String],
    palette_sel: usize,
    git_open: bool,
    git_lines: &'a [String],
    diags: &'a [keplr_lang::Diagnostic],
    extras: &'a [(usize, usize)],
}

fn draw(f: &Frame) -> anyhow::Result<()> {
    let (cols, rows) = terminal::size()?;
    let mut out = stdout();
    execute!(out, MoveTo(0, 0), Clear(ClearType::All))?;
    let dot = if f.dirty { "●" } else { " " };
    execute!(
        out,
        Print(format!(
            "\x1b[1;36mkeplr\x1b[0m {} {dot} \x1b[2m{} {:?}\x1b[0m\r\n",
            f.file_label, f.branch, f.lang
        ))
    )?;
    let mut tabline = String::new();
    for (name, active, dirty) in f.tabs.iter() {
        let d = if *dirty { "●" } else { "" };
        if *active {
            tabline.push_str(&format!("\x1b[7m {name}{d} \x1b[0m "));
        } else {
            tabline.push_str(&format!("\x1b[2m{name}{d}\x1b[0m "));
        }
    }
    execute!(out, Print(format!("{tabline}\r\n")))?;
    let height = rows.saturating_sub(4) as usize;
    if f.git_open {
        execute!(out, Print("\x1b[1mgit status — G/Esc closes\x1b[0m\r\n"))?;
        for line in f.git_lines.iter().take(height.saturating_sub(1)) {
            let shown = truncate_cells(line, cols as usize);
            execute!(out, Print(format!("{shown}\r\n")))?;
        }
    } else {
        for i in 0..height {
            let idx = f.top_render + i;
            if let Some(line) = f.doc.lines.get(idx) {
                let shown =
                    colored_spans(f.lang, line, cols.saturating_sub(7) as usize);
                execute!(
                    out,
                    Print(format!("\x1b[2m{:>4} \x1b[0m{shown}\r\n", idx + 1))
                )?;
                for d in f
                    .diags
                    .iter()
                    .filter(|d| d.line as usize == idx + 1 && d.severity == "error")
                    .take(2)
                {
                    let msg = truncate_cells(&d.message, cols.saturating_sub(12) as usize);
                    execute!(
                        out,
                        Print(format!("\x1b[31m    ~ {}:{}\x1b[0m {msg}\r\n", d.line, d.col))
                    )?;
                }
            } else {
                execute!(out, Print("~\r\n"))?;
            }
        }
    }
    execute!(
        out,
        Print(format!(
            "\x1b[7m {:<w$} \x1b[0m\r\n",
            f.status,
            w = cols.saturating_sub(2) as usize
        ))
    )?;
    if f.palette_open {
        execute!(
            out,
            MoveTo(0, 2),
            Clear(ClearType::CurrentLine),
            Print(format!("› {}\r\n", f.palette_query))
        )?;
        for (i, h) in f.palette_hits.iter().take(8).enumerate() {
            let mark = if i == f.palette_sel { "▸" } else { " " };
            let shown = truncate_cells(h, cols.saturating_sub(4) as usize);
            if i == f.palette_sel {
                execute!(out, Print(format!("\x1b[7m{mark} {shown}\x1b[0m\r\n")))?;
            } else {
                execute!(out, Print(format!("{mark} {shown}\r\n")))?;
            }
        }
    }
    if f.cursor_visible && !f.git_open {
        let crow = 2 + f.cursor.0.saturating_sub(f.top_render);
        let ccol = 5 + f.cursor.1;
        execute!(out, Show, MoveTo(ccol as u16, crow as u16))?;
    } else {
        execute!(out, Hide)?;
    }
    for (line, col) in f.extras.iter() {
        if *line < f.top_render || *line >= f.top_render + height {
            continue;
        }
        let row = 2 + line.saturating_sub(f.top_render);
        let ch = f
            .doc
            .lines
            .get(*line)
            .and_then(|l| l.chars().nth(*col))
            .unwrap_or(' ');
        execute!(out, MoveTo((5 + col) as u16, row as u16))?;
        execute!(out, Print(format!("\x1b[7m{ch}\x1b[0m")))?;
    }
    if !f.extras.is_empty() && f.cursor_visible && !f.git_open {
        let crow = 2 + f.cursor.0.saturating_sub(f.top_render);
        let ccol = 5 + f.cursor.1;
        execute!(out, Show, MoveTo(ccol as u16, crow as u16))?;
    }
    out.flush()?;
    Ok(())
}

fn truncate_cells(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

fn word_before(doc: &Doc, cursor: (usize, usize)) -> (usize, String) {
    let line = doc.lines.get(cursor.0).cloned().unwrap_or_default();
    let chars: Vec<char> = line.chars().collect();
    let mut start = cursor.1.min(chars.len());
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    (start, chars[start..cursor.1.min(chars.len())].iter().collect())
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_fwd(line: &[char], col: usize) -> usize {
    let n = line.len();
    let mut i = col.min(n);
    if i < n && is_word_char(line[i]) {
        while i < n && is_word_char(line[i]) {
            i += 1;
        }
    }
    while i < n && !is_word_char(line[i]) {
        i += 1;
    }
    i.min(n)
}

fn word_back(line: &[char], col: usize) -> usize {
    let mut i = col.min(line.len());
    if i > 0 {
        i -= 1;
        while i > 0 && !is_word_char(line[i]) {
            i -= 1;
        }
        while i > 0 && is_word_char(line[i - 1]) {
            i -= 1;
        }
    }
    i
}

fn word_end(line: &[char], col: usize) -> usize {
    let n = line.len();
    let mut i = (col + 1).min(n);
    while i < n && !is_word_char(line[i]) {
        i += 1;
    }
    while i + 1 < n && is_word_char(line[i + 1]) {
        i += 1;
    }
    i.min(n)
}

fn save_active_tab(
    ws: &keplr_core::Workspace,
    tabs: &mut [TabState],
    active: usize,
) -> String {
    let content = tabs[active].doc.content();
    let full = tabs[active].path.clone();
    match keplr_core::save_buffer(ws, &full, &content) {
        Ok(r) => {
            tabs[active].dirty = false;
            refresh_diags(&mut tabs[active]);
            format!(
                "saved {} hash={} cas={} git={}",
                r.bytes,
                &r.hash[..12.min(r.hash.len())],
                r.cas_stored,
                r.git_committed
            )
        }
        Err(e) => format!("save failed: {e:#}"),
    }
}

pub fn edit_file(
    root: PathBuf,
    file: PathBuf,
    no_animations: bool,
    vim: bool,
) -> anyhow::Result<()> {
    let reduced = reduced_motion(no_animations);
    let ws = keplr_core::Workspace::new(root.clone());
    let branch = keplr_ui::branch_name(&root);
    let mut tabs = vec![open_tab(&root, &file)];
    let mut active = 0usize;
    let mut status = String::from("ctrl+s save · ctrl+p finder · g git · ctrl+q quit");
    let mut quit_armed = false;
    let mut palette_open = false;
    let mut palette_query = String::new();
    let mut palette_hits: Vec<String> = Vec::new();
    let mut palette_sel = 0usize;
    let mut index: Vec<PathBuf> = Vec::new();
    let mut git_open = false;
    let mut git_lines: Vec<String> = Vec::new();
    let mut last_frame = Instant::now();
    let mut last_key = Instant::now();
    let mut blink_on;
    let vim_on = vim;
    let mut vim_normal = vim;
    let mut pending: Option<char> = None;
    let mut clipboard = String::new();
    let mut clip_line = false;
    let mut cmdline: Option<String> = None;
    let mut extras: Vec<(usize, usize)> = Vec::new();
    let _guard = ScreenGuard::enter()?;

    let refresh_palette = |query: &str, index: &[PathBuf], root: &Path| -> Vec<String> {
        if query.is_empty() {
            return index
                .iter()
                .take(20)
                .map(|p| {
                    p.strip_prefix(root)
                        .unwrap_or(p)
                        .display()
                        .to_string()
                })
                .collect();
        }
        keplr_core::search::fuzzy_paths(index, query, 20)
            .into_iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap_or(&p)
                    .display()
                    .to_string()
            })
            .collect()
    };

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f32().min(0.25);
        last_frame = now;
        {
            let tab = &mut tabs[active];
            if reduced {
                tab.top_f = tab.top as f32;
            } else {
                let eased = ease_out_cubic((dt * 14.0).min(1.0));
                tab.top_f += (tab.top as f32 - tab.top_f) * eased;
                if (tab.top as f32 - tab.top_f).abs() < 0.5 {
                    tab.top_f = tab.top as f32;
                }
            }
        }
        let idle_ms = now.duration_since(last_key).as_millis();
        if idle_ms < 530 {
            blink_on = true;
        } else if !reduced {
            blink_on = (idle_ms / 530).is_multiple_of(2);
        } else {
            blink_on = true;
        }
        let animating = (tabs[active].top_f - tabs[active].top as f32).abs() > 0.5;
        let (_, rows) = terminal::size().unwrap_or((100, 30));
        let height = rows.saturating_sub(4) as usize;
        {
            let tab = &mut tabs[active];
            if tab.cursor.0 < tab.top {
                tab.top = tab.cursor.0;
            }
            if tab.cursor.0 >= tab.top + height.max(1) {
                tab.top = tab.cursor.0 - height.max(1) + 1;
            }
            let max_col = tab.doc.line_chars(tab.cursor.0);
            if tab.cursor.1 > max_col {
                tab.cursor.1 = max_col;
            }
        }
        let tab = &tabs[active];
        let labels: Vec<(String, bool, bool)> = tabs
            .iter()
            .enumerate()
            .map(|(i, t)| (t.label.clone(), i == active, t.dirty))
            .collect();
        draw(&Frame {
            doc: &tab.doc,
            file_label: &tab.label,
            branch: &branch,
            lang: tab.lang,
            cursor: tab.cursor,
            cursor_visible: blink_on,
            top_render: tab.top_f.round() as usize,
            dirty: tab.dirty,
            status: &status,
            tabs: &labels,
            palette_open,
            palette_query: &palette_query,
            palette_hits: &palette_hits,
            palette_sel,
            git_open,
            git_lines: &git_lines,
            diags: &tab.diags,
            extras: &extras,
        })?;
        let timeout = if animating && !reduced {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(50)
        };
        if !event::poll(timeout)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        last_key = Instant::now();
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('s') => {
                    status = save_active_tab(&ws, &mut tabs, active);
                    quit_armed = false;
                    continue;
                }
                KeyCode::Char('p') => {
                    palette_open = !palette_open;
                    git_open = false;
                    if palette_open {
                        index = keplr_core::Workspace::new(root.clone())
                            .walk_files(20_000)
                            .into_iter()
                            .map(|e| e.path)
                            .collect();
                        palette_query.clear();
                        palette_sel = 0;
                        palette_hits = refresh_palette("", &index, &root);
                    }
                    continue;
                }
                KeyCode::Char(']') => {
                    active = (active + 1) % tabs.len();
                    extras.clear();
                    continue;
                }
                KeyCode::Char('[') => {
                    active = (active + tabs.len() - 1) % tabs.len();
                    extras.clear();
                    continue;
                }
                KeyCode::Char('q') | KeyCode::Char('c') => {
                    if tabs.iter().any(|t| t.dirty) && !quit_armed {
                        quit_armed = true;
                        status = String::from("unsaved changes — press ctrl+q again");
                        continue;
                    }
                    return Ok(());
                }
                KeyCode::Char('d') => {
                    let tab = &tabs[active];
                    let line = tab.doc.lines.get(tab.cursor.0).cloned().unwrap_or_default();
                    let chars: Vec<char> = line.chars().collect();
                    let mut s = tab.cursor.1.min(chars.len());
                    while s > 0 && is_word_char(chars[s - 1]) {
                        s -= 1;
                    }
                    let mut e = tab.cursor.1.min(chars.len());
                    while e < chars.len() && is_word_char(chars[e]) {
                        e += 1;
                    }
                    if s < e {
                        let word: String = chars[s..e].iter().collect();
                        let at = |ln: usize, from: usize| -> Option<(usize, usize)> {
                            let l = tab.doc.lines.get(ln)?;
                            let cs: Vec<char> = l.chars().collect();
                            if from > cs.len() {
                                return None;
                            }
                            let hay: String = cs[from..].iter().collect();
                            hay.find(word.as_str()).map(|pos| {
                                (ln, from + hay[..pos].chars().count())
                            })
                        };
                        let mut found = None;
                        for ln in tab.cursor.0..tab.doc.lines.len() {
                            let from = if ln == tab.cursor.0 { e } else { 0 };
                            if let Some(p) = at(ln, from) {
                                found = Some(p);
                                break;
                            }
                        }
                        if found.is_none() {
                            for ln in 0..=tab.cursor.0.min(tab.doc.lines.len().saturating_sub(1)) {
                                let end = if ln == tab.cursor.0 { s } else { usize::MAX };
                                let l = match tab.doc.lines.get(ln) {
                                    Some(l) => l,
                                    None => continue,
                                };
                                let cs: Vec<char> = l.chars().collect();
                                let hay: String =
                                    cs[..end.min(cs.len())].iter().collect();
                                if let Some(pos) = hay.find(word.as_str()) {
                                    found = Some((ln, hay[..pos].chars().count()));
                                    break;
                                }
                            }
                        }
                        if let Some(p) = found {
                            if p != tab.cursor && !extras.contains(&p) {
                                extras.push(p);
                                status = format!("{} cursors", extras.len() + 1);
                            }
                        }
                    }
                    continue;
                }
                _ => {}
            }
        }
        quit_armed = false;
        if let Some(cmd) = cmdline.clone() {
            let _ = cmd;
            match key.code {
                KeyCode::Esc => {
                    cmdline = None;
                }
                KeyCode::Enter => {
                    let c = cmdline.take().unwrap_or_default();
                    match c.trim() {
                        "w" => {
                            status = save_active_tab(&ws, &mut tabs, active);
                        }
                        "q" => {
                            if tabs.iter().any(|t| t.dirty) {
                                status = String::from("unsaved changes (use q!)");
                            } else {
                                return Ok(());
                            }
                        }
                        "wq" | "x" => {
                            status = save_active_tab(&ws, &mut tabs, active);
                            if !tabs.iter().any(|t| t.dirty) {
                                return Ok(());
                            }
                        }
                        "q!" => return Ok(()),
                        other => {
                            status = format!("unknown command: {other}");
                        }
                    }
                }
                KeyCode::Backspace => {
                    if let Some(c) = cmdline.as_mut() {
                        c.pop();
                    }
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    if let Some(c) = cmdline.as_mut() {
                        c.push(ch);
                    }
                }
                _ => {}
            }
            continue;
        }
        if vim_on && vim_normal && !palette_open && !git_open {
            let plain = !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT);
            let code = if plain { Some(key.code) } else { None };
            let first = {
                let tab = &tabs[active];
                let line = tab.doc.lines.get(tab.cursor.0).cloned().unwrap_or_default();
                let chars: Vec<char> = line.chars().collect();
                let n = chars.len();
                let cur = (tab.cursor.0, tab.cursor.1.min(n));
                let indent = line
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .count();
                (cur, n, indent)
            };
            let ((ln, col), n, indent) = first;
            let mut consumed = true;
            match (pending.take(), code) {
                (_, Some(KeyCode::Esc)) => {
                    extras.clear();
                }
                (Some('d'), Some(KeyCode::Char('d'))) => {
                    let tab = &mut tabs[active];
                    let line = tab.doc.lines.remove(ln.min(tab.doc.lines.len() - 1));
                    if tab.doc.lines.is_empty() {
                        tab.doc.lines.push(String::new());
                    }
                    clipboard = line + "\n";
                    clip_line = true;
                    tab.cursor.0 = ln.min(tab.doc.lines.len() - 1);
                    tab.cursor.1 = 0;
                    tab.dirty = true;
                }
                (Some('y'), Some(KeyCode::Char('y'))) => {
                    let tab = &tabs[active];
                    clipboard = tab.doc.lines.get(ln).cloned().unwrap_or_default() + "\n";
                    clip_line = true;
                }
                (Some('g'), Some(KeyCode::Char('g'))) => {
                    tabs[active].cursor = (0, 0);
                }
                (Some('d'), Some(KeyCode::Char('w'))) => {
                    let tab = &mut tabs[active];
                    let to = word_fwd(
                        &tab.doc.lines[tab.cursor.0].chars().collect::<Vec<_>>(),
                        tab.cursor.1,
                    );
                    let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                    let end = tab.doc.byte_col(tab.cursor.0, to);
                    tab.doc.lines[tab.cursor.0].drain(byte..end);
                    clipboard = String::new();
                    clip_line = false;
                    tab.dirty = true;
                }
                (Some('d'), Some(KeyCode::Char('$'))) => {
                    let tab = &mut tabs[active];
                    let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                    tab.doc.lines[tab.cursor.0].truncate(byte);
                    tab.dirty = true;
                }
                (Some('y'), Some(KeyCode::Char('w'))) => {
                    let tab = &tabs[active];
                    let chars: Vec<char> = tab.doc.lines[tab.cursor.0].chars().collect();
                    let to = word_fwd(&chars, tab.cursor.1);
                    clipboard = chars[tab.cursor.1.min(n)..to.min(n)].iter().collect();
                    clip_line = false;
                }
                (Some('r'), Some(KeyCode::Char(rc))) => {
                    let tab = &mut tabs[active];
                    if tab.cursor.1 < n {
                        let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                        let next = tab.doc.byte_col(tab.cursor.0, tab.cursor.1 + 1);
                        tab.doc.lines[tab.cursor.0].replace_range(byte..next, &rc.to_string());
                        tab.dirty = true;
                    }
                }
                (None, Some(KeyCode::Char('h'))) => {
                    let tab = &mut tabs[active];
                    if tab.cursor.1 > 0 {
                        tab.cursor.1 -= 1;
                    }
                }
                (None, Some(KeyCode::Char('j'))) => {
                    let tab = &mut tabs[active];
                    tab.cursor.0 = (tab.cursor.0 + 1).min(tab.doc.lines.len() - 1);
                }
                (None, Some(KeyCode::Char('k'))) => {
                    let tab = &mut tabs[active];
                    tab.cursor.0 = tab.cursor.0.saturating_sub(1);
                }
                (None, Some(KeyCode::Char('l'))) => {
                    let tab = &mut tabs[active];
                    tab.cursor.1 = (tab.cursor.1 + 1).min(n);
                }
                (None, Some(KeyCode::Char('w'))) => {
                    let tab = &mut tabs[active];
                    let chars: Vec<char> = tab.doc.lines[tab.cursor.0].chars().collect();
                    tab.cursor.1 = word_fwd(&chars, tab.cursor.1);
                }
                (None, Some(KeyCode::Char('b'))) => {
                    let tab = &mut tabs[active];
                    let chars: Vec<char> = tab.doc.lines[tab.cursor.0].chars().collect();
                    tab.cursor.1 = word_back(&chars, tab.cursor.1);
                }
                (None, Some(KeyCode::Char('e'))) => {
                    let tab = &mut tabs[active];
                    let chars: Vec<char> = tab.doc.lines[tab.cursor.0].chars().collect();
                    tab.cursor.1 = word_end(&chars, tab.cursor.1);
                }
                (None, Some(KeyCode::Char('0'))) => {
                    tabs[active].cursor.1 = 0;
                }
                (None, Some(KeyCode::Char('$'))) => {
                    tabs[active].cursor.1 = n;
                }
                (None, Some(KeyCode::Char('^'))) => {
                    tabs[active].cursor.1 = indent;
                }
                (None, Some(KeyCode::Char('G'))) => {
                    let tab = &mut tabs[active];
                    tab.cursor.0 = tab.doc.lines.len() - 1;
                    tab.cursor.1 = 0;
                }
                (None, Some(KeyCode::Char('i'))) => {
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('a'))) => {
                    let tab = &mut tabs[active];
                    tab.cursor.1 = (tab.cursor.1 + 1).min(n);
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('I'))) => {
                    tabs[active].cursor.1 = indent;
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('A'))) => {
                    tabs[active].cursor.1 = n;
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('o'))) => {
                    let tab = &mut tabs[active];
                    tab.doc.lines.insert(tab.cursor.0 + 1, String::new());
                    tab.cursor.0 += 1;
                    tab.cursor.1 = 0;
                    tab.dirty = true;
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('O'))) => {
                    let tab = &mut tabs[active];
                    tab.doc.lines.insert(tab.cursor.0, String::new());
                    tab.cursor.1 = 0;
                    tab.dirty = true;
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('x'))) => {
                    let tab = &mut tabs[active];
                    if tab.cursor.1 < n {
                        let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                        let next = tab.doc.byte_col(tab.cursor.0, tab.cursor.1 + 1);
                        tab.doc.lines[tab.cursor.0].drain(byte..next);
                        tab.dirty = true;
                    }
                }
                (None, Some(KeyCode::Char('D'))) => {
                    let tab = &mut tabs[active];
                    let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                    clipboard = tab.doc.lines[tab.cursor.0][byte..].to_string();
                    clip_line = false;
                    tab.doc.lines[tab.cursor.0].truncate(byte);
                    tab.dirty = true;
                }
                (None, Some(KeyCode::Char('p'))) => {
                    let tab = &mut tabs[active];
                    if clip_line {
                        tab.doc.lines.insert(
                            tab.cursor.0 + 1,
                            clipboard.trim_end_matches('\n').to_string(),
                        );
                        tab.cursor.0 += 1;
                        tab.cursor.1 = 0;
                    } else {
                        let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                        tab.doc.lines[tab.cursor.0].insert_str(byte, &clipboard);
                        tab.cursor.1 += clipboard.chars().count();
                    }
                    tab.dirty = true;
                }
                (None, Some(KeyCode::Char('P'))) => {
                    let tab = &mut tabs[active];
                    if clip_line {
                        tab.doc.lines.insert(
                            tab.cursor.0,
                            clipboard.trim_end_matches('\n').to_string(),
                        );
                        tab.cursor.1 = 0;
                    } else {
                        let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                        tab.doc.lines[tab.cursor.0].insert_str(byte, &clipboard);
                    }
                    tab.dirty = true;
                }
                (None, Some(KeyCode::Char('d')))
                | (None, Some(KeyCode::Char('y')))
                | (None, Some(KeyCode::Char('g')))
                | (None, Some(KeyCode::Char('r'))) => {
                    pending = match key.code {
                        KeyCode::Char(c) => Some(c),
                        _ => None,
                    };
                }
                (None, Some(KeyCode::Char(':'))) => {
                    cmdline = Some(String::new());
                }
                (None, Some(KeyCode::Char('u'))) => {
                    status = String::from("no undo stack — use git checkout to revert");
                }
                (None, Some(KeyCode::Char('/'))) => {
                    status = String::from("search lives in the finder (ctrl+p)");
                }
                _ => {
                    consumed = false;
                }
            }
            let _ = (ln, col);
            if consumed {
                continue;
            }
        }
        if git_open && !palette_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char('g') => {
                    git_open = false;
                }
                _ => {}
            }
            continue;
        }
        if palette_open {
            match key.code {
                KeyCode::Esc => {
                    palette_open = false;
                }
                KeyCode::Enter => {
                    if let Some(hit) = palette_hits.get(palette_sel).cloned() {
                        let next = root.join(&hit);
                        if let Some(i) = tabs.iter().position(|t| t.path == next) {
                            active = i;
                        } else {
                            tabs.push(open_tab(&root, &next));
                            active = tabs.len() - 1;
                        }
                        extras.clear();
                        status = format!("opened {hit}");
                    }
                    palette_open = false;
                    palette_query.clear();
                }
                KeyCode::Backspace => {
                    palette_query.pop();
                    let all = index.clone();
                    palette_hits = refresh_palette(&palette_query, &all, &root);
                    palette_sel = 0;
                }
                KeyCode::Up => {
                    palette_sel = palette_sel.saturating_sub(1);
                }
                KeyCode::Down => {
                    palette_sel = (palette_sel + 1).min(palette_hits.len().saturating_sub(1));
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    palette_query.push(c);
                    let all = index.clone();
                    palette_hits = refresh_palette(&palette_query, &all, &root);
                    palette_sel = 0;
                }
                _ => {}
            }
            continue;
        }
        match key.code {
            KeyCode::Char('g')
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                git_open = true;
                git_lines = match keplr_core::git::status(&root) {
                    Ok(entries) => {
                        if entries.is_empty() {
                            vec![String::from("(clean)")]
                        } else {
                            entries
                                .into_iter()
                                .take(15)
                                .map(|e| format!("{}{} {}", e.staged, e.unstaged, e.path))
                                .collect()
                        }
                    }
                    Err(e) => vec![format!("git: {e:#}")],
                };
            }
            KeyCode::Left => {
                let tab = &mut tabs[active];
                if tab.cursor.1 > 0 {
                    tab.cursor.1 -= 1;
                } else if tab.cursor.0 > 0 {
                    tab.cursor.0 -= 1;
                    tab.cursor.1 = tab.doc.line_chars(tab.cursor.0);
                }
            }
            KeyCode::Right => {
                let tab = &mut tabs[active];
                if tab.cursor.1 < tab.doc.line_chars(tab.cursor.0) {
                    tab.cursor.1 += 1;
                } else if tab.cursor.0 + 1 < tab.doc.lines.len() {
                    tab.cursor.0 += 1;
                    tab.cursor.1 = 0;
                }
            }
            KeyCode::Up => {
                tabs[active].cursor.0 = tabs[active].cursor.0.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = tabs[active].doc.lines.len().saturating_sub(1);
                tabs[active].cursor.0 = (tabs[active].cursor.0 + 1).min(max);
            }
            KeyCode::Home => {
                tabs[active].cursor.1 = 0;
            }
            KeyCode::End => {
                let c = tabs[active].cursor.0;
                tabs[active].cursor.1 = tabs[active].doc.line_chars(c);
            }
            KeyCode::PageUp => {
                let c = tabs[active].cursor.0;
                tabs[active].cursor.0 = c.saturating_sub(20);
            }
            KeyCode::PageDown => {
                let max = tabs[active].doc.lines.len().saturating_sub(1);
                let c = tabs[active].cursor.0;
                tabs[active].cursor.0 = (c + 20).min(max);
            }
            KeyCode::Enter => {
                let tab = &mut tabs[active];
                let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                let tail = tab.doc.lines[tab.cursor.0][byte..].to_string();
                tab.doc.lines[tab.cursor.0].truncate(byte);
                tab.doc.lines.insert(tab.cursor.0 + 1, tail);
                tab.cursor.0 += 1;
                tab.cursor.1 = 0;
                tab.dirty = true;
                extras.clear();
            }
            KeyCode::Backspace => {
                let tab = &mut tabs[active];
                if tab.cursor.1 > 0 {
                    let mut all = vec![tab.cursor];
                    all.extend(extras.iter().cloned());
                    all.sort();
                    all.dedup();
                    // only cursors with col > 0 delete; others ride along
                    let mut applied: Vec<(usize, usize)> = Vec::new();
                    for cur in &all {
                        if cur.0 >= tab.doc.lines.len() {
                            continue;
                        }
                        let max = tab.doc.line_chars(cur.0);
                        let c = cur.1.min(max);
                        if c > 0 {
                            applied.push((cur.0, c));
                        }
                    }
                    let mut desc = applied.clone();
                    desc.sort_by(|a, b| b.cmp(a));
                    for (line, col) in &desc {
                        let byte = tab.doc.byte_col(*line, *col);
                        let prev = tab.doc.byte_col(*line, col - 1);
                        tab.doc.lines[*line].drain(prev..byte);
                    }
                    let shift = |pos: (usize, usize)| -> (usize, usize) {
                        let n = applied
                            .iter()
                            .filter(|&&(l, cc)| l == pos.0 && cc <= pos.1)
                            .count();
                        (pos.0, pos.1.saturating_sub(n))
                    };
                    let primary = tab.cursor;
                    tab.cursor = shift(primary);
                    let mut fresh = Vec::new();
                    for e in extras.iter() {
                        let p = shift(*e);
                        if p != tab.cursor && !fresh.contains(&p) {
                            fresh.push(p);
                        }
                    }
                    extras = fresh;
                    tab.dirty = true;
                } else if tab.cursor.0 > 0 {
                    let tail = tab.doc.lines.remove(tab.cursor.0);
                    tab.cursor.0 -= 1;
                    tab.cursor.1 = tab.doc.line_chars(tab.cursor.0);
                    tab.doc.lines[tab.cursor.0].push_str(&tail);
                    tab.dirty = true;
                    extras.clear();
                }
            }
            KeyCode::Delete => {
                let tab = &mut tabs[active];
                let max = tab.doc.line_chars(tab.cursor.0);
                if tab.cursor.1 < max {
                    let mut all = vec![tab.cursor];
                    all.extend(extras.iter().cloned());
                    all.sort();
                    all.dedup();
                    let mut applied: Vec<(usize, usize)> = Vec::new();
                    for cur in &all {
                        if cur.0 >= tab.doc.lines.len() {
                            continue;
                        }
                        let max = tab.doc.line_chars(cur.0);
                        let c = cur.1.min(max);
                        if c < max {
                            applied.push((cur.0, c));
                        }
                    }
                    let mut desc = applied.clone();
                    desc.sort_by(|a, b| b.cmp(a));
                    for (line, col) in &desc {
                        let byte = tab.doc.byte_col(*line, *col);
                        let next = tab.doc.byte_col(*line, col + 1);
                        tab.doc.lines[*line].drain(byte..next);
                    }
                    let shift = |pos: (usize, usize)| -> (usize, usize) {
                        let n = applied
                            .iter()
                            .filter(|&&(l, cc)| l == pos.0 && cc < pos.1)
                            .count();
                        (pos.0, pos.1.saturating_sub(n))
                    };
                    let primary = tab.cursor;
                    tab.cursor = shift(primary);
                    let mut fresh = Vec::new();
                    for e in extras.iter() {
                        let p = shift(*e);
                        if p != tab.cursor && !fresh.contains(&p) {
                            fresh.push(p);
                        }
                    }
                    extras = fresh;
                    tab.dirty = true;
                } else if tab.cursor.0 + 1 < tab.doc.lines.len() {
                    let tail = tab.doc.lines.remove(tab.cursor.0 + 1);
                    tab.doc.lines[tab.cursor.0].push_str(&tail);
                    tab.dirty = true;
                    extras.clear();
                }
            }
            KeyCode::Tab => {
                extras.clear();
                let tab = &mut tabs[active];
                let (start, word) = word_before(&tab.doc, tab.cursor);
                if !word.is_empty() {
                    if let Some((expanded, at)) =
                        keplr_lang::expand_snippet(tab.lang, &word)
                    {
                        let line = tab.doc.lines[tab.cursor.0].clone();
                        let schars: Vec<char> = line.chars().collect();
                        let mut newline =
                            String::from_iter(&schars[..start]) + &expanded;
                        let rest: String =
                            schars[tab.cursor.1.min(schars.len())..].iter().collect();
                        newline.push_str(&rest);
                        tab.doc.lines[tab.cursor.0] = newline;
                        tab.cursor.1 = start
                            + at.unwrap_or_else(|| {
                                expanded.chars().count()
                            });
                        tab.dirty = true;
                        status = String::from("expanded snippet");
                        continue;
                    } else {
                        let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                        tab.doc.lines[tab.cursor.0].insert_str(byte, "  ");
                        tab.cursor.1 += 2;
                        tab.dirty = true;
                    }
                } else {
                    let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                    tab.doc.lines[tab.cursor.0].insert_str(byte, "  ");
                    tab.cursor.1 += 2;
                    tab.dirty = true;
                }
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                let tab = &mut tabs[active];
                let mut all = vec![tab.cursor];
                all.extend(extras.iter().cloned());
                all.sort();
                all.dedup();
                for cur in all.iter_mut() {
                    if cur.0 < tab.doc.lines.len() {
                        let max = tab.doc.line_chars(cur.0);
                        cur.1 = cur.1.min(max);
                    }
                }
                let mut desc = all.clone();
                desc.sort_by(|a, b| b.cmp(a));
                for (line, col) in &desc {
                    if *line >= tab.doc.lines.len() {
                        continue;
                    }
                    let byte = tab.doc.byte_col(*line, *col);
                    tab.doc.lines[*line].insert(byte, c);
                }
                let shift = |pos: (usize, usize), targets: &[(usize, usize)]| -> (usize, usize) {
                    let n = targets
                        .iter()
                        .filter(|&&(l, cc)| l == pos.0 && cc <= pos.1)
                        .count();
                    (pos.0, pos.1 + n)
                };
                let primary = tab.cursor;
                tab.cursor = shift(primary, &all);
                let mut fresh = Vec::new();
                for e in extras.iter() {
                    let p = shift(*e, &all);
                    if p != tab.cursor && !fresh.contains(&p) {
                        fresh.push(p);
                    }
                }
                extras = fresh;
                tab.dirty = true;
            }
            KeyCode::Esc => {
                git_open = false;
                extras.clear();
                if vim_on {
                    vim_normal = true;
                }
            }
            _ => {}
        }
        {
            let tab = &tabs[active];
            let mode = if vim_on && vim_normal {
                "[N] "
            } else if vim_on {
                "[I] "
            } else {
                ""
            };
            let multi = if extras.is_empty() {
                String::new()
            } else {
                format!(" ({} cursors)", extras.len() + 1)
            };
            status = format!(
                "{mode}{}:{} {}{}",
                tab.cursor.0 + 1,
                tab.cursor.1 + 1,
                if tab.dirty { "modified" } else { "" },
                multi
            );
        }
    }
}
