use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

fn draw(
    doc: &Doc,
    file_label: &str,
    branch: &str,
    lang: &str,
    cursor: (usize, usize),
    top: usize,
    dirty: bool,
    status: &str,
    palette_open: bool,
    palette_query: &str,
    palette_hits: &[String],
    palette_sel: usize,
) -> anyhow::Result<()> {
    let (cols, rows) = terminal::size()?;
    let mut out = stdout();
    execute!(out, MoveTo(0, 0), Clear(ClearType::All))?;
    let dot = if dirty { "●" } else { " " };
    execute!(
        out,
        Print(format!(
            "\x1b[1;36mkeplr\x1b[0m {file_label} {dot} \x1b[2m{branch} {lang}\x1b[0m\r\n"
        ))
    )?;
    let height = rows.saturating_sub(3) as usize;
    for i in 0..height {
        let idx = top + i;
        if let Some(line) = doc.lines.get(idx) {
            let shown = truncate_cells(line, cols.saturating_sub(7) as usize);
            execute!(
                out,
                Print(format!("\x1b[2m{:>4} \x1b[0m{shown}\r\n", idx + 1))
            )?;
        } else {
            execute!(out, Print("~\r\n"))?;
        }
    }
    execute!(
        out,
        Print(format!(
            "\x1b[7m {:<w$} \x1b[0m\r\n",
            status,
            w = cols.saturating_sub(2) as usize
        ))
    )?;
    if palette_open {
        execute!(
            out,
            MoveTo(0, 1),
            Clear(ClearType::CurrentLine),
            Print(format!("› {palette_query}\r\n"))
        )?;
        for (i, h) in palette_hits.iter().take(8).enumerate() {
            let mark = if i == palette_sel { "▸" } else { " " };
            let shown = truncate_cells(h, cols.saturating_sub(4) as usize);
            if i == palette_sel {
                execute!(out, Print(format!("\x1b[7m{mark} {shown}\x1b[0m\r\n")))?;
            } else {
                execute!(out, Print(format!("{mark} {shown}\r\n")))?;
            }
        }
    }
    let crow = 1 + cursor.0.saturating_sub(top);
    let ccol = 5 + cursor.1;
    execute!(out, MoveTo(ccol as u16, crow as u16))?;
    out.flush()?;
    Ok(())
}

fn truncate_cells(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

pub fn edit_file(root: PathBuf, file: PathBuf) -> anyhow::Result<()> {
    let full = if file.is_absolute() {
        file
    } else {
        root.join(&file)
    };
    let ws = keplr_core::Workspace::new(root.clone());
    let file_label = full
        .strip_prefix(&root)
        .unwrap_or(&full)
        .display()
        .to_string();
    let lang = format!("{:?}", keplr_lang::LangKind::from_path(&full));
    let branch = keplr_ui::branch_name(&root);
    let mut doc = Doc::load(&full);
    let mut cursor = (0usize, 0usize);
    let mut top = 0usize;
    let mut dirty = false;
    let mut status = String::from("ctrl+s save · ctrl+p finder · ctrl+q quit");
    let mut quit_armed = false;
    let mut palette_open = false;
    let mut palette_query = String::new();
    let mut palette_hits: Vec<String> = Vec::new();
    let mut palette_sel = 0usize;
    let mut index: Vec<PathBuf> = Vec::new();
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
        let (_, rows) = terminal::size().unwrap_or((100, 30));
        let height = rows.saturating_sub(3) as usize;
        if cursor.0 < top {
            top = cursor.0;
        }
        if cursor.0 >= top + height.max(1) {
            top = cursor.0 - height.max(1) + 1;
        }
        let max_col = doc.line_chars(cursor.0);
        if cursor.1 > max_col {
            cursor.1 = max_col;
        }
        draw(
            &doc,
            &file_label,
            &branch,
            &lang,
            cursor,
            top,
            dirty,
            &status,
            palette_open,
            &palette_query,
            &palette_hits,
            palette_sel,
        )?;
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('s') => {
                    let content = doc.content();
                    match keplr_core::save_buffer(&ws, &full, &content) {
                        Ok(r) => {
                            dirty = false;
                            quit_armed = false;
                            status = format!(
                                "saved {} hash={} cas={} git={}",
                                r.bytes,
                                &r.hash[..12.min(r.hash.len())],
                                r.cas_stored,
                                r.git_committed
                            );
                        }
                        Err(e) => {
                            status = format!("save failed: {e:#}");
                        }
                    }
                    continue;
                }
                KeyCode::Char('p') => {
                    palette_open = !palette_open;
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
                KeyCode::Char('q') | KeyCode::Char('c') => {
                    if dirty && !quit_armed {
                        quit_armed = true;
                        status = String::from("unsaved changes — press ctrl+q again");
                        continue;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        quit_armed = false;
        if palette_open {
            match key.code {
                KeyCode::Esc => {
                    palette_open = false;
                }
                KeyCode::Enter => {
                    if let Some(hit) = palette_hits.get(palette_sel).cloned() {
                        let next = root.join(&hit);
                        doc = Doc::load(&next);
                        cursor = (0, 0);
                        top = 0;
                        dirty = false;
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
            KeyCode::Left => {
                if cursor.1 > 0 {
                    cursor.1 -= 1;
                } else if cursor.0 > 0 {
                    cursor.0 -= 1;
                    cursor.1 = doc.line_chars(cursor.0);
                }
            }
            KeyCode::Right => {
                if cursor.1 < doc.line_chars(cursor.0) {
                    cursor.1 += 1;
                } else if cursor.0 + 1 < doc.lines.len() {
                    cursor.0 += 1;
                    cursor.1 = 0;
                }
            }
            KeyCode::Up => {
                cursor.0 = cursor.0.saturating_sub(1);
            }
            KeyCode::Down => {
                cursor.0 = (cursor.0 + 1).min(doc.lines.len().saturating_sub(1));
            }
            KeyCode::Home => {
                cursor.1 = 0;
            }
            KeyCode::End => {
                cursor.1 = doc.line_chars(cursor.0);
            }
            KeyCode::PageUp => {
                cursor.0 = cursor.0.saturating_sub(20);
            }
            KeyCode::PageDown => {
                cursor.0 = (cursor.0 + 20).min(doc.lines.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                let byte = doc.byte_col(cursor.0, cursor.1);
                let tail = doc.lines[cursor.0][byte..].to_string();
                doc.lines[cursor.0].truncate(byte);
                doc.lines.insert(cursor.0 + 1, tail);
                cursor.0 += 1;
                cursor.1 = 0;
                dirty = true;
            }
            KeyCode::Backspace => {
                if cursor.1 > 0 {
                    let byte = doc.byte_col(cursor.0, cursor.1);
                    let prev = doc.byte_col(cursor.0, cursor.1 - 1);
                    doc.lines[cursor.0].drain(prev..byte);
                    cursor.1 -= 1;
                    dirty = true;
                } else if cursor.0 > 0 {
                    let tail = doc.lines.remove(cursor.0);
                    cursor.0 -= 1;
                    cursor.1 = doc.line_chars(cursor.0);
                    doc.lines[cursor.0].push_str(&tail);
                    dirty = true;
                }
            }
            KeyCode::Delete => {
                let max = doc.line_chars(cursor.0);
                if cursor.1 < max {
                    let byte = doc.byte_col(cursor.0, cursor.1);
                    let next = doc.byte_col(cursor.0, cursor.1 + 1);
                    doc.lines[cursor.0].drain(byte..next);
                    dirty = true;
                } else if cursor.0 + 1 < doc.lines.len() {
                    let tail = doc.lines.remove(cursor.0 + 1);
                    doc.lines[cursor.0].push_str(&tail);
                    dirty = true;
                }
            }
            KeyCode::Tab => {
                let byte = doc.byte_col(cursor.0, cursor.1);
                doc.lines[cursor.0].insert_str(byte, "  ");
                cursor.1 += 2;
                dirty = true;
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                let byte = doc.byte_col(cursor.0, cursor.1);
                doc.lines[cursor.0].insert(byte, c);
                cursor.1 += 1;
                dirty = true;
            }
            KeyCode::Esc => {}
            _ => {}
        }
        status = format!(
            "{}:{} {}",
            cursor.0 + 1,
            cursor.1 + 1,
            if dirty { "modified" } else { "" }
        );
    }
}
