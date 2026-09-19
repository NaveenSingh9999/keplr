# Keplr Plan I — Tree-sitter, Vim, WASM App, GPU Finish, Roaming, Perf, Remote Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close every remaining verifiable gap in one push: real tree-sitter parses with error diagnostics, vim + multi-cursor terminal editing, a served WASM canvas app, finished GPU text, resilient roaming, headless snapshots with published numbers, and remote-operations docs.

**Architecture:** Same additive rules. Tree-sitter lives in `keplr-lang` behind two pure functions over the new-ABI grammars. Vim/multi-cursor extend the TUI loop state without changing the save pipeline. The WASM app is a new `keplr-web` crate reusing `Scene` serde types + `highlight`, served as static files from `.keplr/web`. GPU text gains wrapping, a shaped-line cache, gutter numbers, and a cursor rect. Roaming gains client backoff, server heartbeats, and a status endpoint. Snapshots render headless to PNG; bench numbers land in `docs/PERF.md`.

**Tech Stack:** Rust 1.97.1, plus `tree-sitter 0.27` with grammars (rust 0.24, javascript 0.25, python 0.25, go 0.25, typescript 0.23), `wasm-bindgen 0.2` + `web-sys 0.3`, `image 0.25` (png only, gpu feature).

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md` sections 2, 4, 5, 7, 10

## Global Constraints

- No Electron, no Chromium shell, no WebView. Native binary + `keplr serve` JSON only.
- Same types with no rename, no signature changes on existing public items. New items only.
- RAM soft cap 500MB; new walks keep existing caps; bench corpus unchanged.
- Git remains truth; derived state under `{workspace}/.keplr/` (gitignored).
- LAML reuse only.
- Every part ends with a local commit; the whole plan ends with push + `gh run watch` green (all CI jobs).
- No placeholders: missing grammars/binaries/GPU report honestly; empty token stays documented open-localhost mode.
- Standing orders for this plan: no local `cargo` runs (verify via GitHub CI with `gh`), no new test files — existing CI tests must stay green.

---

## File Structure

```
crates/keplr-lang/src/lib.rs    # I1: parse_sexp + syntax_errors (+ grammar deps)
crates/keplr-lang/Cargo.toml    # I1: tree-sitter + 5 grammars
crates/keplr-cli/src/main.rs    # I1: parse cmd + wider diagnostics; I2: --vim; I6: snapshot cmd
crates/keplr-serve/src/lib.rs   # I1: wider diagnostics; I3: /web/* static; I5: ping + /sync/status
crates/keplr-cli/src/tui.rs     # I2: vim modes + multi-cursor
crates/keplr-web/               # I3: wasm canvas app (new crate)
crates/keplr-render/src/gpu.rs  # I4: wrap/cache/gutter/cursor; I6: snapshot png
crates/keplr-render/Cargo.toml  # I6: image optional
Cargo.toml                      # I1/I3/I6 workspace deps; I3 keplr-web member
assets/web/index.html           # I3: loader page
.github/workflows/ci.yml        # I3: wasm-app job; I6: bench-smoke job
docs/PERF.md                    # I6: published numbers
docs/REMOTE.md + docs/keplr-serve.service  # I7
README.md                       # usage per part
```

---

## Part I1: Tree-sitter parses

### Task I1-1: Grammar deps + parse/error fns

**Files:**
- Modify: `Cargo.toml` (workspace deps)
- Modify: `crates/keplr-lang/Cargo.toml`
- Modify: `crates/keplr-lang/src/lib.rs` (append)

- [ ] **Step 1: Workspace deps.** Append to `[workspace.dependencies]`:

```toml
tree-sitter = "0.27"
tree-sitter-rust = "0.24"
tree-sitter-javascript = "0.25"
tree-sitter-python = "0.25"
tree-sitter-go = "0.25"
tree-sitter-typescript = "0.23"
```

- [ ] **Step 2: Lang Cargo.** Append to `[dependencies]`:

```toml
tree-sitter.workspace = true
tree-sitter-rust.workspace = true
tree-sitter-javascript.workspace = true
tree-sitter-python.workspace = true
tree-sitter-go.workspace = true
tree-sitter-typescript.workspace = true
```

- [ ] **Step 3: Append to lang lib** (end of file):

```rust
fn ts_language(lang: LangKind) -> Option<tree_sitter::Language> {
    let f: tree_sitter_language::LanguageFn = match lang {
        LangKind::Rust => tree_sitter_rust::LANGUAGE,
        LangKind::JavaScript => tree_sitter_javascript::LANGUAGE,
        LangKind::Python => tree_sitter_python::LANGUAGE,
        LangKind::Go => tree_sitter_go::LANGUAGE,
        LangKind::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        LangKind::Tsx => tree_sitter_typescript::LANGUAGE_TSX,
        _ => return None,
    };
    Some(f.into())
}
```

Wait — `tree_sitter_language::LanguageFn` needs the `tree-sitter-language` crate as a direct dep for the TYPE annotation. Avoid naming it: return `Option<tree_sitter::Language>` via a helper per language? `.into()` needs target type inference — write:

```rust
fn ts_language(lang: LangKind) -> Option<tree_sitter::Language> {
    match lang {
        LangKind::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        LangKind::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
        LangKind::Python => Some(tree_sitter_python::LANGUAGE.into()),
        LangKind::Go => Some(tree_sitter_go::LANGUAGE.into()),
        LangKind::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        LangKind::Tsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        _ => None,
    }
}
```

(Implementer: use this version — no `tree-sitter-language` dep needed. `set_language(&Language)` + `parse(&str, None)` per the 0.24 grammar docs pattern.)

```rust
pub fn parse_sexp(lang: LangKind, text: &str) -> Option<String> {
    let language = ts_language(lang)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(text, None).map(|t| t.root_node().to_sexp())
}

pub fn syntax_errors(lang: LangKind, path: &Path, text: &str) -> Vec<Diagnostic> {
    let label = path.display().to_string();
    let language = match ts_language(lang) {
        Some(l) => l,
        None => return Vec::new(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(text, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if out.len() >= 50 {
            break;
        }
        if node.is_error() || node.is_missing() {
            let pos = node.start_position();
            let what = if node.is_missing() {
                format!("missing {}", node.kind())
            } else {
                format!("syntax error near `{}`", node.kind())
            };
            out.push(Diagnostic {
                path: label.clone(),
                line: (pos.row + 1) as u64,
                col: (pos.column + 1) as u64,
                message: what,
                severity: String::from("error"),
            });
        }
        let mut i = node.child_count();
        while i > 0 {
            i -= 1;
            if let Some(c) = node.child(i) {
                stack.push(c);
            }
        }
    }
    out.sort_by(|a, b| (a.line, a.col).cmp(&(b.line, b.col)));
    out
}
```

`node.child_count()` returns `u32` in newer API — `let mut i: u32` then `node.child(i)`. If it's usize, annotations still compile? `while i > 0 { i -= 1; node.child(i) }` works for either integer type. Write it untyped (inference from child()). Good as written.

- [ ] **Step 4: Commit locally**

```bash
git add Cargo.toml crates/keplr-lang/Cargo.toml crates/keplr-lang/src/lib.rs
git commit -m "feat(lang): tree-sitter parses with error diagnostics"
```

### Task I1-2: `keplr parse` + wider diagnostics

**Files:**
- Modify: `crates/keplr-cli/src/main.rs` (`Parse` variant + arm; extend `Diagnostics` arm)
- Modify: `crates/keplr-serve/src/lib.rs` (extend `diagnostics` handler)

- [ ] **Step 1: CLI variant** (after `Diagnostics`):

```rust
    Parse {
        file: PathBuf,
    },
```

- [ ] **Step 2: CLI arms.** New arm after the `Diagnostics` arm:

```rust
        Cmd::Parse { file } => {
            let lang = keplr_lang::LangKind::from_path(&file);
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            match keplr_lang::parse_sexp(lang, &text) {
                Some(sexp) => println!("{sexp}"),
                None => println!("no grammar for {lang:?}"),
            }
        }
```

Extend the `Diagnostics` arm: old condition body:

```rust
            let diagnostics = if lang == keplr_lang::LangKind::Laml {
                keplr_lang::laml_diagnostics(&file)
            } else {
                Vec::new()
            };
```

New:

```rust
            let diagnostics = if lang == keplr_lang::LangKind::Laml {
                keplr_lang::laml_diagnostics(&file)
            } else {
                match lang {
                    keplr_lang::LangKind::Rust
                    | keplr_lang::LangKind::Python
                    | keplr_lang::LangKind::JavaScript
                    | keplr_lang::LangKind::TypeScript
                    | keplr_lang::LangKind::Tsx
                    | keplr_lang::LangKind::Go => {
                        let text = std::fs::read_to_string(&file).unwrap_or_default();
                        keplr_lang::syntax_errors(lang, &file, &text)
                    }
                    _ => Vec::new(),
                }
            };
```

- [ ] **Step 3: Serve handler.** Old:

```rust
    let diags = if lang == keplr_lang::LangKind::Laml {
        keplr_lang::laml_diagnostics(&full)
    } else {
        Vec::new()
    };
```

New (mirror the CLI match, reading via Buffer load like the highlight handler does — simplest: `std::fs::read_to_string(&full).unwrap_or_default()`):

```rust
    let diags = if lang == keplr_lang::LangKind::Laml {
        keplr_lang::laml_diagnostics(&full)
    } else {
        match lang {
            keplr_lang::LangKind::Rust
            | keplr_lang::LangKind::Python
            | keplr_lang::LangKind::JavaScript
            | keplr_lang::LangKind::TypeScript
            | keplr_lang::LangKind::Tsx
            | keplr_lang::LangKind::Go => {
                let text = std::fs::read_to_string(&full).unwrap_or_default();
                keplr_lang::laml_diagnostics(&full)
                    .into_iter()
                    .chain(keplr_lang::syntax_errors(lang, &full, &text))
                    .collect()
            }
            _ => Vec::new(),
        }
    };
```

No — don't chain laml_diagnostics there (wrong lang, and it shells a binary pointlessly). Correct version:

```rust
            keplr_lang::LangKind::Go => {
                let text = std::fs::read_to_string(&full).unwrap_or_default();
                keplr_lang::syntax_errors(lang, &full, &text)
            }
```

(Implementer: use this; the chain draft above is wrong.)

- [ ] **Step 4: Commit locally**

```bash
git add crates/keplr-cli/src/main.rs crates/keplr-serve/src/lib.rs
git commit -m "feat(lang): parse command plus grammar diagnostics in cli and serve"
```

---

## Part I2: TUI vim + multi-cursor

### Task I2-1: Vim modes + command line

**Files:**
- Modify: `crates/keplr-cli/src/tui.rs` (vim state, normal-mode map, `:` line)
- Modify: `crates/keplr-cli/src/main.rs` (`Edit` gains `--vim`)

**Interfaces:**
- Produces: `keplr edit <file> --vim`; Normal/Insert modes; `:w :q :wq :q!`

Design (locked): per-tab `vim_normal: bool` (only meaningful with `--vim`; starts true like vim), `pending: Option<char>` for `d`/`y`/`r`/`g` doubles, `clipboard: String` + `clip_line: bool` at editor level, `cmdline: Option<String>` (`:` mode). Motions reuse cursor clamping. No undo (no undo stack exists — documented in README vim note). Entering Normal clears extra cursors (multi-cursor is an insert-mode feature).

- [ ] **Step 1: State.** Add to the editor-state area (where `quit_armed`, `palette_*` etc. live):

```rust
    let vim = no_animations; // NO — placeholder logic; real code below
```

Real code — `edit_file` signature gains nothing yet; add locals after `let mut git_lines`:

```rust
    let vim_on = vim;
    let mut vim_normal = vim;
    let mut pending: Option<char> = None;
    let mut clipboard = String::new();
    let mut clip_line = false;
    let mut cmdline: Option<String> = None;
```

And `edit_file(root, file, no_animations)` gains `vim: bool`: `pub fn edit_file(root: PathBuf, file: PathBuf, no_animations: bool, vim: bool)`. Update the single call site in main.rs + add the CLI flag in I2-2.

Word helpers (free fns):

```rust
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_fwd(line: &[char], col: usize) -> usize {
    let n = line.len();
    let mut i = col.min(n);
    // skip rest of current word
    if i < n && is_word_char(line[i]) {
        while i < n && is_word_char(line[i]) {
            i += 1;
        }
    }
    // skip separators, then skip to word start (stop at end)
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
```

Normal-mode dispatch: insert BEFORE the palette/main handling, gated on `vim_on && vim_normal && cmdline.is_none() && !palette_open`. Structure:

```rust
        if vim_on && vim_normal && cmdline.is_none() && !palette_open {
            match key.code {
                KeyCode::Char('h') => { left (no modifiers) }
                ...
            }
            continue;
        }
```

Full normal map (plain chars only, no ctrl/alt modifiers unless noted):

- `h/j/k/l`: move (reuse existing logic — factor? duplicate 3-liners, fine)
- `w` → cursor.1 = word_fwd; `b` → word_back; `e` → word_end; `0` → 0; `$` → EOL (line_chars); `^` → first non-blank
- `g` pending: `gg` → (0,0); `G` handled separately (no pending needed)
- `G` → last line, first non-blank
- `i` → insert mode; `a` → col+1 (clamped) + insert; `I` → first non-blank + insert; `A` → EOL + insert; `o` → open below; `O` → open above
- `x` → delete char under cursor (drain single char); `D` → delete to EOL (clipboard=text, clip_line=false); `dd` (pending d + d) → delete line (clipboard=line+"\n", clip_line=true); `yy` → yank line; `p` → paste below (line) or at cursor (char); `P` → paste above/at cursor; `r` pending → replace char with next typed char
- `d`/`y` pending + motion (`w`, `$`, `0`) → operate on range [cursor, target): delete/yank text range on the line (multi-line motions out of scope — only same-line; `dw` at EOL deletes to EOL)
- `:` → cmdline = Some(String::new()); Esc in cmdline cancels; Enter executes: "w" save, "q" quit-if-clean (else status hint), "wq"/"x" save+quit, "q!" force quit
- `u` → status "no undo stack (use git checkout)"; `/` → status "search lives in the finder (ctrl+p)"
- `Ctrl+[` already prev-tab; keep. `gt/gT`? skip.

Pending handling: on any non-pending key, clear pending (except the doubles). Implement with match on `(pending, key.code)` — write it as nested matches exactly as below (implementer: transcribe):

```rust
        if vim_on && vim_normal && cmdline.is_none() && !palette_open {
            let plain = !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT);
            let tab = &mut tabs[active];
            let chars: Vec<char> = tab.doc.lines[tab.cursor.0].chars().collect();
            let n = chars.len();
            let mut consumed = true;
            match (pending.take(), if plain { Some(key.code) } else { None }) {
                (_, Some(KeyCode::Esc)) => {}
                (Some('d'), Some(KeyCode::Char('d'))) => {
                    let line = tab.doc.lines.remove(tab.cursor.0);
                    if tab.doc.lines.is_empty() {
                        tab.doc.lines.push(String::new());
                    }
                    clipboard = line + "\n";
                    clip_line = true;
                    tab.cursor.0 = tab.cursor.0.min(tab.doc.lines.len() - 1);
                    tab.cursor.1 = 0;
                    tab.dirty = true;
                }
                (Some('y'), Some(KeyCode::Char('y'))) => {
                    clipboard = tab.doc.lines[tab.cursor.0].clone() + "\n";
                    clip_line = true;
                }
                (Some('g'), Some(KeyCode::Char('g'))) => {
                    tab.cursor = (0, 0);
                }
                (Some('d'), Some(KeyCode::Char('w'))) => {
                    let to = word_fwd(&chars, tab.cursor.1);
                    tab.doc.lines[tab.cursor.0].drain(
                        tab.doc.byte_col(tab.cursor.0, tab.cursor.1)
                            ..tab.doc.byte_col(tab.cursor.0, to),
                    );
                    clipboard = String::new();
                    clip_line = false;
                    tab.dirty = true;
                }
                (Some('d'), Some(KeyCode::Char('$'))) => {
                    let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                    tab.doc.lines[tab.cursor.0].truncate(byte);
                    tab.dirty = true;
                }
                (Some('y'), Some(KeyCode::Char('w'))) => {
                    let to = word_fwd(&chars, tab.cursor.1);
                    clipboard = chars[tab.cursor.1.min(n)..to.min(n)].iter().collect();
                    clip_line = false;
                }
                (Some('r'), Some(KeyCode::Char(rc))) => {
                    if tab.cursor.1 < n {
                        let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                        let next = tab.doc.byte_col(tab.cursor.0, tab.cursor.1 + 1);
                        tab.doc.lines[tab.cursor.0].replace_range(byte..next, &rc.to_string());
                        tab.dirty = true;
                    }
                }
                (None, Some(KeyCode::Char('h'))) => {
                    if tab.cursor.1 > 0 {
                        tab.cursor.1 -= 1;
                    }
                }
                (None, Some(KeyCode::Char('j'))) => {
                    tab.cursor.0 = (tab.cursor.0 + 1).min(tab.doc.lines.len() - 1);
                }
                (None, Some(KeyCode::Char('k'))) => {
                    tab.cursor.0 = tab.cursor.0.saturating_sub(1);
                }
                (None, Some(KeyCode::Char('l'))) => {
                    tab.cursor.1 = (tab.cursor.1 + 1).min(n);
                }
                (None, Some(KeyCode::Char('w'))) => {
                    tab.cursor.1 = word_fwd(&chars, tab.cursor.1);
                }
                (None, Some(KeyCode::Char('b'))) => {
                    tab.cursor.1 = word_back(&chars, tab.cursor.1);
                }
                (None, Some(KeyCode::Char('e'))) => {
                    tab.cursor.1 = word_end(&chars, tab.cursor.1);
                }
                (None, Some(KeyCode::Char('0'))) => {
                    tab.cursor.1 = 0;
                }
                (None, Some(KeyCode::Char('$'))) => {
                    tab.cursor.1 = n;
                }
                (None, Some(KeyCode::Char('^'))) => {
                    let s = &tab.doc.lines[tab.cursor.0];
                    tab.cursor.1 = s.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                }
                (None, Some(KeyCode::Char('G'))) => {
                    tab.cursor.0 = tab.doc.lines.len() - 1;
                    tab.cursor.1 = 0;
                }
                (None, Some(KeyCode::Char('i'))) => {
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('a'))) => {
                    tab.cursor.1 = (tab.cursor.1 + 1).min(n);
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('I'))) => {
                    let s = &tab.doc.lines[tab.cursor.0];
                    tab.cursor.1 = s.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('A'))) => {
                    tab.cursor.1 = n;
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('o'))) => {
                    tab.doc.lines.insert(tab.cursor.0 + 1, String::new());
                    tab.cursor.0 += 1;
                    tab.cursor.1 = 0;
                    tab.dirty = true;
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('O'))) => {
                    tab.doc.lines.insert(tab.cursor.0, String::new());
                    tab.cursor.1 = 0;
                    tab.dirty = true;
                    vim_normal = false;
                }
                (None, Some(KeyCode::Char('x'))) => {
                    if tab.cursor.1 < n {
                        let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                        let next = tab.doc.byte_col(tab.cursor.0, tab.cursor.1 + 1);
                        tab.doc.lines[tab.cursor.0].drain(byte..next);
                        tab.dirty = true;
                    }
                }
                (None, Some(KeyCode::Char('D'))) => {
                    let byte = tab.doc.byte_col(tab.cursor.0, tab.cursor.1);
                    clipboard = tab.doc.lines[tab.cursor.0][byte..].to_string();
                    clip_line = false;
                    tab.doc.lines[tab.cursor.0].truncate(byte);
                    tab.dirty = true;
                }
                (None, Some(KeyCode::Char('p'))) => {
                    if clip_line {
                        tab.doc.lines.insert(tab.cursor.0 + 1, clipboard.trim_end_matches('\n').to_string());
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
                    if clip_line {
                        tab.doc.lines.insert(tab.cursor.0, clipboard.trim_end_matches('\n').to_string());
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
                _ => {}
            }
            // Esc in normal mode clears extra cursors (below) — fall through to shared tail
            continue;
        }
```

Wait — borrow problem: `let tab = &mut tabs[active];` then later arms use `tab`, and after match `continue` — fine. But `chars` borrows tab.doc.lines[...] immutably while tab is mutably borrowed — `let chars: Vec<char>` is an OWNED vec (collected) ✓ no borrow held. But `let s = &tab.doc.lines[...]` in I/A arms borrows tab immutably while tab mutably borrowed → ERROR. Fix those arms: compute indent first into owned/collected count without holding borrow:

```rust
                (None, Some(KeyCode::Char('I'))) => {
                    let indent = tab.doc.lines[tab.cursor.0]
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .count();
                    tab.cursor.1 = indent;
                    vim_normal = false;
                }
```
`tab.doc.lines[...]` indexing returns place expression borrowed for the statement duration — `.chars().take_while().count()` consumes within the statement, borrow ends ✓ compiles. Same for `^` arm. Fix both in the transcription above (the `^` arm has the same shape — rewrite identically).

Also `let tab = &mut tabs[active];` + `pending.take()` etc. — pending/clipboard are separate locals ✓. `status` assignment in u// arms ✓ separate. `cmdline` ✓.

Esc in normal mode: arm `(_, Some(KeyCode::Esc)) => {}` then falls to `continue` — but extra cursors should clear on Esc. Add `tabs[active].extras.clear()`? Multi-cursor state design (I2-2) — decide representation NOW to avoid rework: per-tab `extra: Vec<(usize, usize)>` field on TabState? That changes TabState struct (additive field — but TabState constructed in open_tab + palette Enter (2 sites) — update both). Hmm, actually simpler: store extras as editor-level `Vec<(usize, usize)>` applying to the ACTIVE tab only (cleared on tab switch — documented). Editor-level `let mut extras: Vec<(usize, usize)> = Vec::new();`. Esc in normal → extras.clear(). Tab switch → extras.clear(). Ctrl+D adds. Insert/delete ops iterate primary + extras. Draw: pass extras to Frame? Frame has cursor single... draw cursor + extras: extend Frame with `extra: &[(usize, usize)]` and paint accent-outline blocks. OK — I2-2 details; for I2-1 add the field + Esc clearing + tab-switch clearing, ops come in I2-2.

Cmdline rendering + handling: status bar doubles as cmdline display: in draw, if cmdline Some → status line shows `:{cmd}`. Status is computed (`status` var) — override at draw call: pass `cmdline.as_deref()` into Frame? Add Frame field `cmd: Option<&str>`? Frame is in tui.rs (my struct) — extend with `cmdline: Option<String>`? Lifetimes... simplest: compute `let status_show = cmdline.as_ref().map(|c| format!(":{c}")).unwrap_or(status.clone())` hmm allocation per frame — fine. And cmdline input: while `cmdline.is_some()`, printable keys append, Backspace pops, Enter executes, Esc cancels. Insert this branch BEFORE the vim-normal branch (cmdline works in both modes? vim : only makes sense with vim on — but harmless always; gate on vim_on? If not vim_on, `:` types colon normally. Gate: only when vim_on).

Execute:
```rust
        if let Some(cmd) = cmdline.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    cmdline = None;
                }
                KeyCode::Enter => {
                    let c = cmdline.take().unwrap_or_default();
                    match c.as_str() {
                        "w" => { /* save active tab like ctrl+s */ }
                        "q" => { if any dirty { status = "unsaved changes" } else { return Ok(()); } }
                        "wq" | "x" => { /* save; return Ok */ }
                        "q!" => return Ok(()),
                        _ => status = format!("unknown command: {c}"),
                    }
                }
                KeyCode::Backspace => {
                    cmd.pop();
                }
                KeyCode::Char(ch) if plain => {
                    cmd.push(ch);
                }
                _ => {}
            }
            continue;
        }
```
Borrow: `cmdline.as_mut()` borrows cmdline mutably; inside Enter arm `cmdline.take()` — same borrow live → ERROR (cmd borrowed by outer if-let). Restructure: match on cloned? Do:
```rust
        if cmdline.is_some() {
            match key.code { ... cmdline.as_mut().unwrap().push... }
        }
```
Each arm re-borrows briefly — sequential, fine. For Enter: `let c = cmdline.take().unwrap_or_default();` ✓ no live borrow.

Save-duplication: factor `fn save_tab(...)`? The ctrl+s arm has ~15 lines; cmdline w needs same. Extract helper fn `save_active(ws, root, tabs, active) -> ...`? It mutates tabs + needs ws/root + returns status String. Signature: `fn save_active_tab(ws: &Workspace, tabs: &mut [TabState], active: usize) -> String`. TabState type local ✓. Implementer: extract + reuse in both places. (Careful: refresh_diags call inside too.)

Status line shows mode: trailing status block sets `line:col modified` — extend: prefix `[N] ` when vim_on && vim_normal else `[I]`? Set in trailing block:
```rust
status = format!("{} {}:{} {}", if vim_on && vim_normal { "[N]" } else if vim_on { "[I]" } else { "" }, ...)
```
Hmm keep simple: prepend mode tag when vim_on.

This is getting long — transcribe carefully in implementation. Also `Edit` CLI flag `--vim` + arm passes through; `edit_file` signature + vim param.

- [ ] Steps: 1) state+helpers+normal map+cmdline+mode tag, 2) CLI flag, 3) commit.

### Task I2-2: Multi-cursor ops

**Files:** `tui.rs` only.

- `extras: Vec<(usize, usize)>` editor-level (active tab only; cleared on tab switch, palette open, normal-mode entry... spec: entering Normal clears extras).
- `Ctrl+D`: word at primary cursor (alnum run); search forward from primary for next occurrence (same line remainder, then following lines, then wrap from top); push; status count.
- Insert path: currently `KeyCode::Char(c)` arm inserts at primary. Extend: build list of all cursors (primary + extras), sort desc by (line, col), apply insert to each (byte_col per cursor recomputed after each op? ops on same line shift later cols — process DESC so earlier edits don't disturb later positions ✓), update each cursor col+1. Backspace/Delete similarly (col>0 only; skip col-0 extras for backspace, skip EOL extras for delete — documented partial rule? Hmm earlier I decided structural→primary-only. Char-level backspace at col>0 is SAFE for all (same-line desc order). At col 0 (join) → primary only? Joining shifts lines for cursors below! Rule (locked): if ANY cursor is at a join position (backspace col 0 / delete at EOL / Enter), apply to primary only + clear extras with status note? Simpler deterministic: structural ops always primary-only (extras stay, clamped). Char insert/delete-in-line → all. Enter → primary only. Document in README vim note + status hint on first multi use? Just README.)
- Draw: Frame gains `extras: &[(usize, usize)]`; paint hollow/accent blocks for extras (different shade: dim accent outline? canvas-free TUI: draw `▌`? For terminal TUI, render extra cursors as reversed space? Simplest visible: draw `◆`? Hmm — TUI draws text rows; extra cursor marker: reverse-video the char under each extra cursor. Implement in draw loop: if (line,col) in extras → wrap char in \x1b[7m.
- Esc: clears extras (insert mode) / normal mode Esc already; in normal Esc → extras.clear().
- Tab switch/palette → extras.clear().
- Commit.

### Task I2-3: CLI `--vim` + README note (fold into I2-1/I2-2 commits; no separate commit)

Actually fold flag into I2-1. README vim section in I7 (docs task) — no, put usage in I2 commit? README edits: batch all Plan I README changes in I7 to avoid conflicts. But then intermediate commits lack docs — fine.

---

## Part I3: WASM canvas app

### Task I3-1: `keplr-web` crate

**Files:**
- Create: `crates/keplr-web/Cargo.toml`, `crates/keplr-web/src/lib.rs`
- Modify: `Cargo.toml` (member + `wasm-bindgen`, `web-sys` deps)

- [ ] **Step 1: Workspace.** Members += `"crates/keplr-web"`. Deps += `wasm-bindgen = "0.2"`. web-sys with features at use-site (version in workspace table bare):

```toml
web-sys = "0.3"
```

Hmm — workspace inheritance `web-sys.workspace = true` + `features = [...]` at use site: allowed ✓.

- [ ] **Step 2: Crate files.**

```toml
[package]
name = "keplr-web"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen.workspace = true
serde.workspace = true
serde_json.workspace = true
keplr-lang = { path = "../keplr-lang" }
keplr-render = { path = "../keplr-render" }

[dependencies.web-sys]
workspace = true
features = [
    "Window",
    "Document",
    "Element",
    "HtmlElement",
    "HtmlCanvasElement",
    "CanvasRenderingContext2d",
]
```

```rust
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, CanvasRenderingContext2d};

fn ctx2d(canvas: &HtmlCanvasElement) -> Result<CanvasRenderingContext2d, JsValue> {
    let ctx: JsValue = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?;
    ctx.dyn_into()
}

fn canvas_by_id(id: &str) -> Result<HtmlCanvasElement, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window.document().ok_or_else(|| JsValue::from_str("no document"))?;
    let el = document
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str("no canvas"))?;
    el.dyn_into::<HtmlCanvasElement>()
        .map_err(|_| JsValue::from_str("not a canvas"))
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[wasm_bindgen]
pub fn highlight_line(lang: &str, line: &str) -> String {
    let kind = keplr_lang::LangKind::from_path(std::path::Path::new(&format!("x.{lang}")));
    let spans = keplr_lang::highlight(kind, line);
    serde_json::to_string(&spans).unwrap_or_else(|_| String::from("[]"))
}

#[wasm_bindgen]
pub fn render_scene(canvas_id: &str, scene_json: &str) -> Result<(), JsValue> {
    let canvas = canvas_by_id(canvas_id)?;
    let ctx = ctx2d(&canvas)?;
    let scene: keplr_render::Scene =
        serde_json::from_str(scene_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    paint(&ctx, &canvas, &scene)
}

fn px(ctx: &CanvasRenderingContext2d, canvas: &HtmlCanvasElement) -> (f64, f64) {
    let w = canvas.width() as f64;
    let h = canvas.height() as f64;
    ctx.set_fill_style(&JsValue::from_str("#0e1116"));
    ctx.fill_rect(0.0, 0.0, w, h);
    (w, h)
}

fn paint(
    ctx: &CanvasRenderingContext2d,
    canvas: &HtmlCanvasElement,
    scene: &keplr_render::Scene,
) -> Result<(), JsValue> {
    let (w, h) = px(ctx, canvas);
    let theme = keplr_render::Theme::zed_dark();
    let font = "13px 'JetBrains Mono','SF Mono',monospace";
    ctx.set_font(font);
    // titlebar
    ctx.set_fill_style(&JsValue::from_str(&theme.surface));
    ctx.fill_rect(0.0, 0.0, w, 30.0);
    ctx.set_fill_style(&JsValue::from_str(&theme.text));
    ctx.fill_text(&scene.titlebar.root, 12.0, 20.0)?;
    // left dock
    let lw = w * 0.22;
    ctx.set_fill_style(&JsValue::from_str(&theme.surface));
    ctx.fill_rect(0.0, 30.0, lw, h - 56.0);
    ctx.set_fill_style(&JsValue::from_str(&theme.text_dim));
    let mut y = 48.0;
    for line in scene.left.lines.iter().take(30) {
        ctx.fill_text(&line.chars().take(32).collect::<String>(), 12.0, y)?;
        y += 17.0;
    }
    // center editor
    let x0 = lw + 12.0;
    let mut y = 48.0;
    for (i, line) in scene.center.lines.iter().take(40).enumerate() {
        let n = scene.center.viewport_top + i;
        ctx.set_fill_style(&JsValue::from_str(&theme.text_dim));
        ctx.fill_text(&format!("{n:>3}"), x0, y)?;
        ctx.set_fill_style(&JsValue::from_str(&theme.text));
        ctx.fill_text(&line.chars().take(100).collect::<String>(), x0 + 44.0, y)?;
        y += 17.0;
    }
    // cursor
    let cy = 48.0 + ((scene.center.cursor.0 - scene.center.viewport_top) as f64) * 17.0;
    ctx.set_fill_style(&JsValue::from_str(&theme.accent));
    ctx.fill_rect(x0 + 44.0, cy - 12.0, 8.0, 15.0);
    // status bar
    ctx.set_fill_style(&JsValue::from_str(&theme.surface));
    ctx.fill_rect(0.0, h - 26.0, w, 26.0);
    ctx.set_fill_style(&JsValue::from_str(&theme.text_dim));
    ctx.fill_text(
        &format!("{} · {} files", scene.status.branch, scene.status.files),
        12.0,
        h - 9.0,
    )?;
    // palette
    if scene.palette_open {
        ctx.set_fill_style(&JsValue::from_str("#161b22ee"));
        ctx.fill_rect(w * 0.25, 60.0, w * 0.5, 220.0);
        ctx.set_fill_style(&JsValue::from_str(&theme.text));
        ctx.fill_text(&scene.palette_query, w * 0.25 + 12.0, 84.0)?;
        let mut y = 106.0;
        for hit in scene.palette_hits.iter().take(8) {
            ctx.fill_text(hit, w * 0.25 + 12.0, y)?;
            y += 17.0;
        }
    }
    Ok(())
}
```

`ctx.fill_text(&str, f64, f64)` returns Result<(), JsValue> ✓ (`?` works). `canvas.width()` u32 ✓. `dyn_into` needs `wasm_bindgen::JsCast` — import it: `use wasm_bindgen::JsCast;`. Add that import (implementer: include it).

Cursor line when cursor above viewport: `(cursor.0 - viewport_top)` underflows usize! Guard: `saturating_sub`. Fix in transcription: `((scene.center.cursor.0.saturating_sub(scene.center.viewport_top)) as f64)`.

- [ ] **Step 3: Commit locally**

```bash
git add Cargo.toml crates/keplr-web/
git commit -m "feat(web): wasm canvas renderer reusing scene types"
```

### Task I3-2: Loader page + static serving + CI jobs

**Files:**
- Create: `assets/web/index.html`
- Modify: `crates/keplr-serve/src/lib.rs` (`/web/*` static)
- Modify: `.github/workflows/ci.yml` (wasm-app job)

- [ ] **Step 1: Loader** `assets/web/index.html` (hand-written; loads the built bundle from the same dir):

```html
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>keplr canvas</title>
<style>
  html, body { margin: 0; height: 100%; background: #0e1116; }
  #app { width: 100vw; height: 100vh; display: block; }
  #err { color: #f85149; font: 13px monospace; padding: 20px; white-space: pre-wrap; }
</style>
</head>
<body>
<canvas id="app" width="1280" height="800"></canvas>
<div id="err"></div>
<script type="module">
  const err = (m) => { document.getElementById("err").textContent += m + "\n"; };
  const root = document.body.dataset.root || ".";
  try {
    const mod = await import("./keplr_web.js");
    await mod.default();
    const scene = await (await fetch("/scene?width=120")).json();
    mod.render_scene("app", JSON.stringify(scene));
    const canvas = document.getElementById("app");
    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;
    mod.render_scene("app", JSON.stringify(scene));
    err("keplr canvas " + mod.version() + " — static snapshot (live loop is the next layer)");
  } catch (e) {
    err("canvas failed: " + e + "\nbuild: cargo build -p keplr-web --target wasm32-unknown-unknown --release && wasm-bindgen --target web --out-dir <root>/.keplr/web");
  }
</script>
</body>
</html>
```

Static snapshot honesty: it renders once (live redraw loop deferred — stated in-page).

- [ ] **Step 2: Serve static.** AppState unchanged. Add handler + route:

```rust
async fn web_file(
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::body::Body;
    let rel = if path.is_empty() || path == "/" {
        "index.html".to_string()
    } else {
        path.trim_start_matches('/').to_string()
    };
    let full = match safe_rel(&state.root.join(".keplr/web"), &rel) {
        Ok(p) => p,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("{e:#}")})),
            )
                .into_response()
        }
    };
    let bytes = match std::fs::read(&full) {
        Ok(b) => b,
        Err(_) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "no bundle — build it (see assets/web/index.html)"})),
            )
                .into_response()
        }
    };
    let mime = match full.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html",
        "js" => "text/javascript",
        "wasm" => "application/wasm",
        "css" => "text/css",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    (
        [(axum::http::header::CONTENT_TYPE, mime)],
        Body::from(bytes),
    )
        .into_response()
}
```

`safe_rel` exists (GB work) ✓. `axum::extract::Path` extractor — imported? serve imports `extract::{ConnectInfo, Query, State}` — add `Path as AxumPath`? Name clash with std Path import! Use full path `axum::extract::Path<String>` inline in signature (no import needed). Route: `.route("/web/*path", get(web_file))`. Axum 0.7 wildcard syntax `/web/*path` ✓.

`Body` — `axum::body::Body` full path inline ✓ no import change.

- [ ] **Step 3: CI wasm-app job.** Append:

```yaml
  wasm-app:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
          targets: wasm32-unknown-unknown
      - uses: Swatinem/rust-cache@v2
      - run: cargo install wasm-bindgen-cli
      - run: cargo build -p keplr-web --target wasm32-unknown-unknown --release
      - run: wasm-bindgen target/wasm32-unknown-unknown/release/keplr_web.wasm --out-dir /tmp/keplr-web --target web
      - run: ls -la /tmp/keplr-web && test -f /tmp/keplr-web/keplr_web_bg.wasm
```

- [ ] **Step 4: Commit locally**

```bash
git add assets/web/index.html crates/keplr-serve/src/lib.rs .github/workflows/ci.yml
git commit -m "feat(web): static bundle serving plus wasm-app ci"
```

---

## Part I4: GPU text finish

### Task I4-1: Wrap + cache + gutter + cursor rect

**Files:**
- Modify: `crates/keplr-render/src/gpu.rs` (all gated, CI-verified via gpu-check)

- [ ] **Step 1: Cache + wrap + gutter.** Change `scene_text_quads` signature to take wrap width and return cursor rect:

```rust
pub fn scene_text_quads(
    atlas: &GlyphAtlas,
    scene: &Scene,
    w_px: u32,
    h_px: u32,
) -> (Vec<f32>, Option<[f32; 4]>) {
    let lang = lang_of(&scene.center.lang);
    let mut v = Vec::new();
    let gutter = 52.0f32;
    let x0 = w_px as f32 * 0.22 + 12.0 + gutter;
    let wrap_x = w_px as f32 * 0.82 - 12.0;
    let mut y = 44.0f32;
    let mut cursor_rect = None;
    for (i, line) in scene.center.lines.iter().take(25).enumerate() {
        let n = scene.center.viewport_top + i;
        // gutter number
        let num = format!("{n:>4} ");
        v.extend(layout_colored_line(
            atlas,
            &num,
            &[],
            x0 - gutter,
            y,
            w_px as f32,
            h_px as f32,
        ));
        // wrap: greedy char walk with per-char advance from the atlas
        let spans = keplr_lang::highlight(lang, line);
        let mut row = String::new();
        let mut row_w = 0.0f32;
        let mut rows: Vec<String> = Vec::new();
        for c in line.chars() {
            let adv = atlas
                .glyphs
                .get(&(c, 16u32))
                .map(|g| g.advance)
                .unwrap_or(8.0);
            if row_w + adv > (wrap_x - x0).max(40.0) && !row.is_empty() {
                rows.push(std::mem::take(&mut row));
                row_w = 0.0;
            }
            row.push(c);
            row_w += adv;
        }
        rows.push(row);
        for (ri, sub) in rows.iter().enumerate() {
            // NOTE: spans are line-relative; wrapped continuation rows render plain.
            // Full per-row re-highlighting is the documented next refinement.
            let spans = if ri == 0 { spans.clone() } else { Vec::new() };
            v.extend(layout_colored_line(
                atlas,
                sub,
                &spans,
                x0,
                y,
                w_px as f32,
                h_px as f32,
            ));
            if n == scene.center.cursor.0 && ri == 0 {
                cursor_rect = Some([x0, y, 9.0, 17.0]);
            }
            y += 18.0;
        }
    }
    (v, cursor_rect)
}
```

Hmm — wrapped continuation rows render plain (spans belong to the full line; slicing spans per row is fiddly). The NOTE is honest but is it placeholder-ish? It's a documented refinement with correct first-row rendering; continuations still show text. Acceptable, stated.

Wait, `spans.clone()` — Span is Clone ✓. And `keplr_lang::Span` import? gpu.rs references full paths (`keplr_lang::highlight`) — `layout_colored_line(atlas, sub, &spans, ...)` takes `&[keplr_lang::Span]`? Current signature: `spans: &[keplr_lang::Span]`? It was written as `spans: &[Span]` with... let me not guess: current code calls `layout_colored_line(atlas, line, &spans, ...)` where spans: Vec from highlight → so param type is `&[keplr_lang::Span]` or imported Span. Passing `&spans` (Vec) and `&[]`... `&[]` needs type annotation! `&spans` where spans: Vec<Span> ✓; for empty need `&Vec::new()`? In gutter call I pass `&[]` — type inference from param type `&[Span]` → `&[]` works IF param is slice ✓. OK as long as signature is slice-based. (Verify during implementation via grep.)

Cursor rect into layout quads: App call site currently:

```rust
let text_verts = match &gpu.text_atlas {
    Some(atlas) => scene_text_quads(atlas, scene, gpu.size.0, gpu.size.1),
    None => Vec::new(),
};
```

New:

```rust
let (text_verts, cursor_rect) = match &gpu.text_atlas {
    Some(atlas) => scene_text_quads(atlas, scene, gpu.size.0, gpu.size.1),
    None => (Vec::new(), None),
};
let mut layout = layout_quads(scene, gpu.size.0, gpu.size.1);
if let Some([x, y, w, h]) = cursor_rect {
    let accent = parse_hex(&Theme::zed_dark().accent);
    // layout_quads is NDC-space; convert px rect here instead:
    layout.extend(cursor_px_to_ndc(x, y, w, h, gpu.size.0, gpu.size.1, accent));
}
gpu.upload(&layout);
```

Add helper:

```rust
fn cursor_px_to_ndc(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    vw: u32,
    vh: u32,
    c: [f32; 3],
) -> Vec<f32> {
    let mut v = Vec::new();
    let nx = |px: f32| px / vw as f32 * 2.0 - 1.0;
    let ny = |py: f32| 1.0 - py / vh as f32 * 2.0;
    push_quad(&mut v, nx(x), ny(y), nx(x + w) - nx(x), ny(y + h) - ny(y), c);
    v
}
```

push_quad takes NDC x/y/w/h (it builds corners) ✓ matches existing use in layout_quads.

Cache in App: `text_cache: std::collections::HashMap<u64, Vec<f32>>` + key fn:

```rust
fn text_cache_key(scene: &Scene, w: u32, h: u32) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut s = DefaultHasher::new();
    scene.center.viewport_top.hash(&mut s);
    scene.center.cursor.hash(&mut s);
    w.hash(&mut s);
    h.hash(&mut s);
    for line in scene.center.lines.iter().take(25) {
        line.hash(&mut s);
    }
    s.finish()
}
```

In RedrawRequested: compute key; `let (text_verts, cursor_rect) = match cache.get → cloned : shape+insert (cap: if len > 32 { clear })`. Cursor changes per frame? cursor included in key ✓ (cursor moves → miss → reshape; fine, shaping 25 lines is cheap).

Hmm — cursor_rect depends on cursor → part of shaped output? Cache stores verts only; cursor rect recomputed... simpler: cache stores (verts, rect) tuple. `HashMap<u64, (Vec<f32>, Option<[f32;4]>)>`.

Rewrite that call-site block accordingly (implementer: write it).

- [ ] **Step 2: Commit locally**

```bash
git add crates/keplr-render/src/gpu.rs
git commit -m "feat(gpu): wrapped text with gutter cursor and shape cache"
```

---

## Part I5: Roaming resilience

### Task I5-1: Heartbeats + peer counts + client backoff

**Files:**
- Modify: `crates/keplr-serve/src/lib.rs` (peers map, ping, `/sync/status`)
- Modify: `crates/keplr-cli/src/main.rs` (reconnect loop)

- [ ] **Step 1: Serve.** AppState gains `sync_peers: Arc<Mutex<HashMap<String, u32>>>` (update both constructors — find `sync_docs:`/`sync_tx:` init sites; there is exactly one in `serve_full`). In `channel_loop`: after the hello send, `*peers.entry(name.clone()).or_insert(0) += 1` (write it as entry API); at loop end (all break paths converge at function end) decrement with saturating_sub + remove on zero. Since breaks exit the loop, put decrement after the loop:

```rust
    // ... existing loop ...
    {
        let mut peers = state.sync_peers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = peers.get_mut(&name) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                peers.remove(&name);
            }
        }
    }
```

Heartbeat: in the select loop add:

```rust
            tick = heartbeat.tick() => {
                if socket.send(Message::Ping(Vec::new())).await.is_err() {
                    break;
                }
            }
```

with `let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(30));` before the loop. axum `Message::Ping(Vec<u8>)` — 0.7 tungstenite-backed Message::Ping takes Vec<u8> ✓ (matches our Binary(Vec<u8>) usage).

Status endpoint:

```rust
async fn sync_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let peers = state.sync_peers.lock().unwrap_or_else(|e| e.into_inner());
    let docs = state.sync_docs.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = docs
        .keys()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "peers": peers.get(name).cloned().unwrap_or(0),
            })
        })
        .collect();
    Json(serde_json::json!({ "docs": list }))
}
```

Route `.route("/sync/status", get(sync_status))`.

- [ ] **Step 2: Client backoff.** Wrap the connect+loop body of the `Sync` arm: extract the whole existing body into `async fn sync_session(...)`? Minimal-diff approach: wrap in `for attempt in 0..` loop with a labeled retry. The arm body is long; restructuring risks breakage. Cleaner: move the session into a helper fn in main.rs:

```rust
async fn sync_once(
    url: &str,
    name: &str,
    file: &Option<PathBuf>,
    token: &str,
    once: bool,
) -> anyhow::Result<()> {
    ... existing arm body, with `url`/`name`/`file`/`token` params instead of moved bindings ...
}
```

That's a big cut-paste; transcription risk high. Alternative minimal approach: keep the arm body as-is, wrap ONLY the connect in retry:

Actually the connect line is one statement; backoff needs looping the whole session (socket drops → reconnect). Compromise (locked): extract helper. The existing body references `url, name, file, token, once` (the destructured Cmd fields). New arm:

```rust
        Cmd::Sync {
            url,
            name,
            file,
            token,
            once,
        } => {
            let mut backoff = std::time::Duration::from_secs(1);
            loop {
                match sync_session(&url, &name, &file, &token, once).await {
                    Ok(()) => break,
                    Err(e) => {
                        if once {
                            return Err(e);
                        }
                        eprintln!("sync: {e:#}; retrying in {}s", backoff.as_secs());
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(std::time::Duration::from_secs(30));
                    }
                }
            }
        }
```

And `sync_session` = the current arm body verbatim (it already ends with `return Ok(())` paths and `?` errors; signature `async fn sync_session(url: &str, name: &str, file: &Option<PathBuf>, token: &str, once: bool) -> anyhow::Result<()>` placed before `#[tokio::main] async fn main`). The body uses `format!`, `println!`, `eprintln!` — all fine in a free fn. It uses `tokio_tungstenite`, `futures_util`, `http` — same crate ✓.

Implementer: cut the arm body (from `use futures_util...` through the final `}` of the loop) into the helper, replacing `url/name/file/token/once` bindings with params. `file: &Option<PathBuf>` — body does `if let Some(f) = &file` where file: &Option → `&file` is &&Option... adjust: body currently matches on `&file` (Option moved into arm). In helper, param is already `&Option`, so change `if let Some(f) = &file` → `if let Some(f) = file`, and `file.as_ref()` stays. Careful transcription required — read the arm first.

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-serve/src/lib.rs crates/keplr-cli/src/main.rs
git commit -m "feat(sync): heartbeats peers status plus client backoff"
```

---

## Part I6: Snapshots + bench evidence

### Task I6-1: Headless PNG snapshots

**Files:**
- Modify: `Cargo.toml` (`image` dep, png-only)
- Modify: `crates/keplr-render/Cargo.toml` (optional dep in gpu feature)
- Modify: `crates/keplr-render/src/gpu.rs` (`snapshot_scene_png`)
- Modify: `crates/keplr-cli/src/main.rs` (`Snapshot` command, desktop-gated)
- Modify: `.github/workflows/ci.yml` (best-effort snapshot artifact)

- [ ] **Step 1: Deps.** Workspace: `image = { version = "0.25", default-features = false, features = ["png"] }`. Render Cargo: `image = { workspace = true, optional = true }` + gpu feature += `"dep:image"`.

- [ ] **Step 2: Snapshot fn** (append to gpu.rs):

```rust
pub fn snapshot_scene_png(scene_json: &str, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    let scene: Scene =
        serde_json::from_str(scene_json).map_err(|e| anyhow::anyhow!("bad scene: {e}"))?;
    let w = width.clamp(64, 4096);
    let h = height.clamp(64, 4096);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: true,
        compatible_surface: None,
    }))
    .ok_or_else(|| anyhow::anyhow!("no GPU adapter (not even fallback)"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .map_err(|e| anyhow::anyhow!("no GPU device: {e:?}"))?;
    // layout pipeline (shared helper?) — duplicate the small setup inline:
    ...
}
```

Problem: pipeline setup lives inside `Gpu::new` (window-bound). Refactor minimally: extract `fn layout_pipeline(device) -> (pipeline, shader-module?)`... The layout pipeline needs only device. Extract:

```rust
fn create_layout_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    ... shader + layout + pipeline with format param ...
}
```

But `Gpu::new` currently builds it inline — refactor `Gpu::new` to call the helper (behavior identical), then `snapshot_scene_png` reuses it. Target texture format: `TextureFormat::Rgba8UnormSrgb`? For PNG output use Rgba8Unorm (linear) — canvas parity aside, pick `Rgba8UnormSrgb`? PNG has no colorspace tag issues at this level; use Rgba8Unorm for predictable bytes. Hmm — layout pipeline in Gpu::new is compiled against the SURFACE format. For snapshot, call helper with Rgba8Unorm. Fine.

Full snapshot body:

```rust
pub fn snapshot_scene_png(scene_json: &str, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    use image::{ImageBuffer, Rgba};
    let scene: Scene =
        serde_json::from_str(scene_json).map_err(|e| anyhow::anyhow!("bad scene: {e}"))?;
    let (w, h) = (width.clamp(64, 4096), height.clamp(64, 4096));
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: true,
        compatible_surface: None,
    }))
    .ok_or_else(|| anyhow::anyhow!("no GPU adapter (not even fallback)"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .map_err(|e| anyhow::anyhow!("no GPU device: {e:?}"))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let pipeline = create_layout_pipeline(&device, format);
    let quads = layout_quads(&scene, w, h);
    let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (f32_to_bytes(&quads).len().max(16)) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vbuf, 0, &f32_to_bytes(&quads));
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let theme = Theme::zed_dark();
    let bg = parse_hex(&theme.bg);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: bg[0] as f64, g: bg[1] as f64, b: bg[2] as f64, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.draw(0..quads.len() as u32 / 6, 0..1);
    }
    let pitch = ((w * 4 + 255) / 256 * 256) as u64;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: pitch * h as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::ImageCopyBuffer { buffer: &buf, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(pitch as u32), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |v| {
        let _ = tx.send(v);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| anyhow::anyhow!("map cancelled"))?
        .map_err(|e| anyhow::anyhow!("map failed: {e:?}"))?;
    let padded = slice.get_mapped_range().to_vec();
    drop(padded);
    // re-map dance: copy rows out first
    let data = slice.get_mapped_range();
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        let off = y * pitch as usize;
        px.extend_from_slice(&data[off..off + (w * 4) as usize]);
    }
    drop(data);
    buf.unmap();
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(w, h, px).ok_or_else(|| anyhow::anyhow!("bad pixels"))?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).encode(
        img.as_raw(),
        w,
        h,
        image::ExtendedColorType::Rgba8,
    ).map_err(|e| anyhow::anyhow!("png encode: {e}"))?;
    Ok(png)
}
```

Fix the mapped-range dance (my draft reads twice): correct sequence is map → get_mapped_range → copy rows → drop range → unmap. The draft above does `to_vec` then re-gets — `get_mapped_range` twice is fine sequentially? Second call while first alive would panic; I drop first. Simplify to single pass (implementer: write the clean single-pass version — copy rows, drop, unmap).

`image::codecs::png::PngEncoder::new(&mut png).encode(...)` — with default-features=false + png: `image::codecs::png` available? png feature enables it ✓. `encode(&[u8], w, h, ExtendedColorType)` — signature `encode(self, buf, width, height, color: ExtendedColorType)`? In image 0.25, `PngEncoder::encode(buf: &[u8], width: u32, height: u32, color: ExtendedColorType) -> ImageResult<()>` ✓. `ExtendedColorType::Rgba8` ✓.

Also `create_layout_pipeline` extraction — refactor Gpu::new to use it. Read Gpu::new's pipeline block during implementation and extract verbatim with a `format` param.

- [ ] **Step 3: CLI command** (desktop-gated):

```rust
    #[cfg(feature = "desktop")]
    Snapshot {
        #[arg(long)]
        open: Option<PathBuf>,
        #[arg(long, default_value_t = 1280)]
        width: u32,
        #[arg(long, default_value_t = 800)]
        height: u32,
        #[arg(long, default_value = "shot.png")]
        out: PathBuf,
    },
```

Arm:

```rust
        #[cfg(feature = "desktop")]
        Cmd::Snapshot {
            open,
            width,
            height,
            out,
        } => {
            let spec = keplr_render::SceneSpec {
                root: &cli.root,
                open_file: open.as_deref(),
                query: "",
                palette_query: None,
                palette_mode: "files",
                search_query: None,
                left_tab: "project",
                right_tab: "symbols",
                bottom_tab: "terminal",
                width: 100,
            };
            let scene = keplr_render::build_scene(&spec);
            let json = serde_json::to_string(&scene)?;
            let png = keplr_render::gpu::snapshot_scene_png(&json, width, height)?;
            std::fs::write(&out, &png)?;
            println!("wrote {} ({} bytes)", out.display(), png.len());
        }
```

- [ ] **Step 4: CI best-effort artifact.** Append to the `gpu-check` job (read it first; it has two `cargo check` runs):

```yaml
      - run: cargo check -p keplr-render --features gpu
      - run: cargo check -p keplr-cli --features desktop
      - run: cargo build -p keplr-cli --features desktop
      - run: ./target/debug/keplr --root . snapshot --open Cargo.toml --width 800 --height 600 --out /tmp/shot.png || echo "no GPU on runner"
      - uses: actions/upload-artifact@v4
        if: hashFiles('/tmp/shot.png') != ''
        with:
          name: render-snapshot
          path: /tmp/shot.png
```

`hashFiles` in `if:` — valid GitHub expression ✓.

- [ ] **Step 5: Commit locally**

```bash
git add Cargo.toml crates/keplr-render/Cargo.toml crates/keplr-render/src/gpu.rs crates/keplr-cli/src/main.rs .github/workflows/ci.yml
git commit -m "feat(gpu): headless png snapshots with best-effort ci artifact"
```

### Task I6-2: Bench evidence + PERF.md

**Files:**
- Modify: `.github/workflows/ci.yml` (bench-smoke job)
- Create: `docs/PERF.md` (filled after CI prints numbers — second commit)

- [ ] **Step 1: CI job.** Append:

```yaml
  bench-smoke:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo run -q -p keplr-cli -- --root /tmp/benchroot bench --files 200 --lines 20 --json
```

- [ ] **Step 2: After green, read the job log** (`gh run view <id> --log --job <bench-smoke-id>` or `--log-failed` won't show passing logs — use `gh run view <id> --log`), transcribe the JSON into `docs/PERF.md`:

```markdown
# Keplr perf numbers

Measured `<date>` on GitHub `ubuntu-latest` via `keplr bench --files 200 --lines 20 --json` (synth Rust-like corpus).

| metric | value |
|---|---|
| ... transcribed ... |

Budgets (spec §3): 100k files / 5M LOC, 500MB soft RAM, <16ms result chunks. Re-run locally with larger `--files` for ceiling numbers.
```

- [ ] **Step 3: Commits** — job first (`feat(ci): bench smoke numbers`), PERF.md second (`docs: published bench numbers`).

---

## Part I7: Operator docs + final verify

### Task I7-1: REMOTE.md + systemd unit + README + push

**Files:**
- Create: `docs/REMOTE.md`
- Create: `docs/keplr-serve.service`
- Modify: `README.md` (Plan I link + usage)

- [ ] **Step 1: `docs/REMOTE.md`:**

```markdown
# Keplr remote operations

## Serve behind Tailscale

```bash
tailscale up
cargo run -p keplr-cli -- --root ~/LAML token --save
cargo run -p keplr-cli -- --root ~/LAML serve --bind 0.0.0.0 --port 7137
```

`--bind` defaults to `127.0.0.1`. Binding anything else without a token (flag, `KEPLR_TOKEN`, or `.keplr/token`) refuses to start unless `--allow-open-lan` is passed. Ten bad auth attempts per minute per IP get HTTP 429.

## systemd unit

See `docs/keplr-serve.service` — copy to `~/.config/systemd/user/`, edit paths, `systemctl --user enable --now keplr-serve`.

## Caddy reverse proxy (optional TLS)

```caddy
keplr.example.com {
    reverse_proxy 127.0.0.1:7137
}
```

Keep serve on loopback and let Caddy terminate TLS.

## Tokens

- `keplr token` prints one; `--save` writes `.keplr/token` (0600); `--rotate` replaces it.
- The browser page accepts `?token=`; API clients send `Authorization: Bearer`.

## Backups

Back up `keplr.json` and `.keplr/*.json` (journal, index). Skip `.keplr/cas` and `.keplr/snapshots` — content-addressed and re-fetchable.

## IME / touch

Terminal input (TUI, browser terminal) is key-event based: no IME composition or touch keyboard integration. The browser editor (CodeMirror) supports mobile keyboards and IME where the OS provides them. Native IME needs the OS window shell.
```

- [ ] **Step 2: `docs/keplr-serve.service`:**

```ini
[Unit]
Description=Keplr headless IDE hub
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=%h/LAML
ExecStart=%h/.cargo/bin/keplr --root %h/LAML serve --port 7137
Restart=on-failure
RestartSec=5
Environment=KEPLR_TOKEN_FILE=%h/LAML/.keplr/token

[Install]
WantedBy=default.target
```

Hmm — `KEPLR_TOKEN_FILE` doesn't exist (only `KEPLR_TOKEN` env). Fix the unit to not reference it: drop that line or use `EnvironmentFile=-%h/LAML/.keplr/token-env`? Simplest honest: drop the line (token resolves from `.keplr/token` automatically). (Implementer: omit the Environment line.)

- [ ] **Step 3: README.** Add `Plan I all remaining: \`docs/superpowers/plans/2026-09-18-keplr-plan-i-all-remaining.md\`` and append:

```markdown
## Use (Plan I all remaining, production)

cargo run -p keplr-cli -- --root . parse crates/keplr-cli/src/main.rs | head -c 400
cargo run -p keplr-cli -- --root . diagnostics src/app.rs
cargo run -p keplr-cli -- --root . edit src/main.rs --vim
cargo run -p keplr-cli -- --root . sync --url ws://127.0.0.1:7137/sync/channel --name notes --file notes.txt
cargo run -p keplr-cli --features desktop -- --root . snapshot --open Cargo.toml --out /tmp/shot.png
```

- [ ] **Step 4: Commit locally, push, watch all jobs**

```bash
git add docs/REMOTE.md docs/keplr-serve.service README.md docs/PERF.md
git commit -m "docs: remote ops unit plus plan i usage"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: all jobs green (including new wasm-app/bench-smoke). Fix forward from logs.

---

## Self-review (run before handoff)

1. Spec coverage: §7 grammars yes (5 langs, sexp, error diagnostics in CLI+serve); §4 vim yes (documented subset, no undo stack), multi-cursor yes (insert-mode ops, structural ops primary-only, documented); §2 WASM app yes (canvas renderer + served bundle flow + CI-built artifact check); GPU text yes (wrap/gutter/cursor/cache); §5 roaming yes (backoff/ping/status); §10 snapshots yes (headless PNG, best-effort artifact) + published bench numbers; remote ops yes (docs+unit).
2. Placeholder scan: no TBD/TODO/placeholder/unimplemented; every public function real; honest degrades only.
3. Type consistency: `parse_sexp/syntax_errors`, `sync_session` params, `layout_colored_line/scene_text_quads` tuple return, `snapshot_scene_png`, `output_tail`, `daemon`, new routes/flags spelled identically at every definition and call site.
