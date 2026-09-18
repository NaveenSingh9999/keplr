# Keplr Plan G — Languages, Snippets, Git, TUI Depth, Security, GPU Text Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the UI/UX depth pass in one push: 25 languages with highlight/LSP data, snippet expansion, a git manager, a highlighted/animated/tabbed terminal editor, hardened serve security with supervised daemons, and a real GPU glyph stack on a licensed font — with WASM opened as a verified slice.

**Architecture:** Same additive rules. `LangKind` grows by 17 variants (no renames); comment markers become a per-language function so `#`/`--`/`<!--` languages highlight correctly. Snippets are static per-language tables with `$0`/`${n:text}` markers expanded by a pure function consumed by the TUI and a serve endpoint. Git is a new `pub mod git` in `keplr-core` shelling the real `git` binary with NUL-safe parsing, surfaced in CLI/serve/TUI. The TUI gains span-colored rendering, file tabs, eased scrolling, cursor blink with reduced-motion respect, snippet Tab-expansion, and a git overlay. Serve gains `--bind`, open-LAN refusal, failure rate-limiting, token rotation, lifetime-supervised daemon tasks, and a cached-log endpoint. The GPU feature gains an `ab_glyph` atlas over discovered fonts.

**Tech Stack:** Rust 1.97.1, existing workspace deps plus `ab_glyph 0.2` (render `gpu` feature only). No other new crates.

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md` sections 2 (font stack), 4 (editor/UX), 5 (hub/LFS), 6 (task logs), 7 (languages)

## Global Constraints

- No Electron, no Chromium shell, no WebView. Native binary + `keplr serve` JSON only.
- Same types with no rename except additive extension: `LangKind` keeps all 8 variants and gains 17; `TaskDef` gains defaulted `daemon`; `JournalEntry` gains defaulted `output_tail`.
- RAM soft cap 500MB: symbol/highlight caps stand; atlas caps at 1024px.
- Git remains truth; derived state under `{workspace}/.keplr/` (gitignored).
- LAML reuse only: shell to `laml` binary if present, never reimplement evaluator. Same reuse principle for `git`/`curl`/`gunzip` binaries.
- Every part ends with a local commit; the whole plan ends with push + `gh run watch` green (default job + `gpu-check` + new `wasm-check`).
- No placeholders: every function real; GPU-less paths fall back honestly; missing binaries report install hints.
- Standing orders for this plan: no local `cargo` runs (verify via GitHub CI with `gh`), no new test files — existing CI tests must stay green.

---

## File Structure

```
assets/fonts/JetBrainsMono-Regular.ttf + -Bold.ttf  # vendored OFL fonts (LFS)
crates/keplr-lang/src/lib.rs   # GA: 17 LangKind variants, keywords, comment markers, LSP table, symbols, install; GB: snippets; wasm cfgs
crates/keplr-render/src/lib.rs # GA: lang_label arms; GE: font_stack/discover_font
crates/keplr-render/src/gpu.rs # GE: ab_glyph atlas + texture upload
crates/keplr-render/Cargo.toml # GE: + ab_glyph optional
crates/keplr-core/src/git.rs   # GB: git manager (new module)
crates/keplr-core/src/lib.rs   # GB: pub mod git
crates/keplr-cli/src/main.rs   # GB: git/snippets/lsp-install/fonts cmds; GC: edit --no-animations; GD: serve --bind/--allow-open-lan
crates/keplr-cli/src/tui.rs    # GC: tabs/highlight/squiggles/animations/snippets/git overlay
crates/keplr-serve/src/lib.rs  # GB: /git/* /snippets; GD: bind/refusal/rate-limit/daemons/log; (existing token gate stays)
crates/keplr-build/src/lib.rs  # GD: daemon flag + output_tail
.github/workflows/ci.yml       # GE: wasm-check job
README.md                      # font decision + usage per part
```

## Font decision (locked)

Apple's SF Mono ships under Apple's license with macOS/Xcode only and cannot be vendored. The stack therefore prefers SF Mono when present on the machine, then the vendored open-licensed JetBrains Mono (OFL, `assets/fonts/`, also the spec's own "JetBrains-Mono fallback stack"), then system monos. `keplr fonts` prints the resolved stack. No proprietary bytes in the repo, ever.

---

## Part GA: 17 more languages + install + wasm slice

### Task GA-1: LangKind expansion + keywords + comments + LSP + symbols

**Files:**
- Modify: `crates/keplr-lang/src/lib.rs` (enum, from_path, keywords, comment fn, highlight uses, lsp_servers, symbols_for)
- Modify: `crates/keplr-render/src/lib.rs` (lang_label arms)

**Interfaces:**
- Consumes: nothing new
- Produces: 17 new `LangKind` variants with full routing; `pub fn line_comment(lang: LangKind) -> Option<&'static str>`; extended `lsp_servers`; `pub fn install_hint(name: &str) -> String`; `pub fn install_rust_analyzer(dest_dir: &Path) -> anyhow::Result<PathBuf>`

New variants and routing (extension → variant): `py→Python`, `c→C`, `cs→CSharp`, `java→Java`, `swift→Swift`, `kt|kts→Kotlin`, `rb→Ruby`, `php→Php`, `html|htm→Html`, `css→Css`, `json→Json`, `toml→Toml`, `yaml|yml→Yaml`, `md|markdown→Markdown`, `sh|bash→Shell`, `sql→Sql`, `lua→Lua`.

- [ ] **Step 1: Extend the enum.** Old:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangKind {
    TypeScript,
    Tsx,
    JavaScript,
    Cpp,
    Go,
    Rust,
    Laml,
    Other,
}
```

New (append before `Other`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangKind {
    TypeScript,
    Tsx,
    JavaScript,
    Cpp,
    Go,
    Rust,
    Laml,
    Python,
    C,
    CSharp,
    Java,
    Swift,
    Kotlin,
    Ruby,
    Php,
    Html,
    Css,
    Json,
    Toml,
    Yaml,
    Markdown,
    Shell,
    Sql,
    Lua,
    Other,
}
```

- [ ] **Step 2: Extend `from_path`.** Old arms end with:

```rust
            "rs" => Self::Rust,
            "lm" => Self::Laml,
            _ => Self::Other,
```

New:

```rust
            "rs" => Self::Rust,
            "lm" => Self::Laml,
            "py" => Self::Python,
            "c" => Self::C,
            "cs" => Self::CSharp,
            "java" => Self::Java,
            "swift" => Self::Swift,
            "kt" | "kts" => Self::Kotlin,
            "rb" => Self::Ruby,
            "php" => Self::Php,
            "html" | "htm" => Self::Html,
            "css" => Self::Css,
            "json" => Self::Json,
            "toml" => Self::Toml,
            "yaml" | "yml" => Self::Yaml,
            "md" | "markdown" => Self::Markdown,
            "sh" | "bash" => Self::Shell,
            "sql" => Self::Sql,
            "lua" => Self::Lua,
            _ => Self::Other,
```

- [ ] **Step 3: Extend `keywords`.** Add arms before `LangKind::Other => &[]`:

```rust
        LangKind::Python => &[
            "def", "class", "return", "if", "else", "elif", "for", "while",
            "import", "from", "as", "try", "except", "finally", "raise",
            "with", "lambda", "pass", "break", "continue", "in", "is",
            "not", "and", "or", "None", "True", "False", "self", "async",
            "await",
        ],
        LangKind::C => &[
            "int", "float", "double", "char", "bool", "void", "struct",
            "enum", "typedef", "union", "static", "const", "extern",
            "return", "if", "else", "for", "while", "do", "switch",
            "case", "break", "continue", "sizeof", "true", "false",
            "NULL", "include",
        ],
        LangKind::CSharp => &[
            "class", "interface", "enum", "struct", "namespace", "using",
            "public", "private", "protected", "internal", "static",
            "virtual", "override", "abstract", "sealed", "return", "if",
            "else", "for", "foreach", "while", "new", "var", "async",
            "await", "try", "catch", "finally", "throw", "true", "false",
            "null", "this",
        ],
        LangKind::Java => &[
            "class", "interface", "enum", "package", "import", "public",
            "private", "protected", "static", "final", "abstract",
            "extends", "implements", "return", "if", "else", "for",
            "while", "new", "try", "catch", "finally", "throw",
            "throws", "true", "false", "null", "this", "void", "int",
        ],
        LangKind::Swift => &[
            "func", "class", "struct", "enum", "protocol", "extension",
            "import", "let", "var", "return", "if", "else", "for",
            "while", "guard", "switch", "case", "break", "continue",
            "in", "as", "is", "try", "catch", "throw", "throws",
            "async", "await", "true", "false", "nil", "self",
        ],
        LangKind::Kotlin => &[
            "fun", "class", "interface", "object", "val", "var", "return",
            "if", "else", "for", "while", "when", "import", "package",
            "try", "catch", "finally", "throw", "true", "false", "null",
            "this", "is", "in", "as",
        ],
        LangKind::Ruby => &[
            "def", "class", "module", "end", "return", "if", "else",
            "elsif", "for", "while", "do", "require", "include",
            "yield", "break", "next", "true", "false", "nil", "self",
            "begin", "rescue", "ensure", "raise",
        ],
        LangKind::Php => &[
            "function", "class", "interface", "trait", "namespace",
            "use", "return", "if", "else", "elseif", "for", "foreach",
            "while", "new", "echo", "try", "catch", "finally", "throw",
            "true", "false", "null", "this",
        ],
        LangKind::Html => &[
            "html", "head", "body", "div", "span", "script", "style",
            "table", "form", "input", "button", "class", "href",
        ],
        LangKind::Css => &[
            "color", "background", "margin", "padding", "border",
            "display", "position", "width", "height", "font",
        ],
        LangKind::Json => &["true", "false", "null"],
        LangKind::Toml => &["true", "false"],
        LangKind::Yaml => &["true", "false", "null"],
        LangKind::Markdown => &[],
        LangKind::Shell => &[
            "if", "then", "else", "elif", "fi", "for", "while", "do",
            "done", "case", "esac", "function", "return", "break",
            "continue", "in", "export", "local", "echo", "true",
            "false",
        ],
        LangKind::Sql => &[
            "select", "from", "where", "join", "left", "right",
            "inner", "outer", "on", "group", "order", "by", "having",
            "insert", "into", "values", "update", "set", "delete",
            "create", "table", "alter", "drop", "and", "or", "not",
            "null", "as", "limit",
        ],
        LangKind::Lua => &[
            "function", "local", "return", "if", "then", "else",
            "elseif", "end", "for", "while", "do", "break", "in",
            "true", "false", "nil",
        ],
```

- [ ] **Step 4: Comment markers.** Add after `keywords`:

```rust
pub fn line_comment(lang: LangKind) -> Option<&'static str> {
    match lang {
        LangKind::Laml => Some("~"),
        LangKind::Html => Some("<!--"),
        LangKind::Python
        | LangKind::Ruby
        | LangKind::Shell
        | LangKind::Toml
        | LangKind::Yaml => Some("#"),
        LangKind::Lua | LangKind::Sql => Some("--"),
        LangKind::Markdown | LangKind::Json | LangKind::Css => None,
        _ => Some("//"),
    }
}
```

And rework the two comment checks in `highlight`. Old:

```rust
        if lang == LangKind::Laml && b == b'~' {
            push_other(&mut spans, &mut other_start, i);
            spans.push(Span {
                start: i,
                len: bytes.len() - i,
                kind: TokenKind::Comment,
            });
            break;
        }
        if lang != LangKind::Laml
            && b == b'/'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'/'
        {
            push_other(&mut spans, &mut other_start, i);
            spans.push(Span {
                start: i,
                len: bytes.len() - i,
                kind: TokenKind::Comment,
            });
            break;
        }
```

New:

```rust
        if let Some(marker) = line_comment(lang) {
            let mb = marker.as_bytes();
            if i + mb.len() <= bytes.len() && &bytes[i..i + mb.len()] == mb {
                push_other(&mut spans, &mut other_start, i);
                spans.push(Span {
                    start: i,
                    len: bytes.len() - i,
                    kind: TokenKind::Comment,
                });
                break;
            }
        }
```

- [ ] **Step 5: Extend `lsp_servers`.** Old tail:

```rust
        LangKind::Rust => &[("rust-analyzer", "rust-analyzer", &[])],
        LangKind::Laml | LangKind::Other => &[],
```

New:

```rust
        LangKind::Rust => &[("rust-analyzer", "rust-analyzer", &[])],
        LangKind::Python => &[("pyright", "pyright", &["--stdio"])],
        LangKind::C => &[("clangd", "clangd", &["--background-index"])],
        LangKind::CSharp => &[("csharp-ls", "csharp-language-server", &[])],
        LangKind::Java => &[("jdtls", "jdtls", &[])],
        LangKind::Swift => &[("sourcekit-lsp", "sourcekit-lsp", &[])],
        LangKind::Kotlin => &[(
            "kotlin-language-server",
            "kotlin-language-server",
            &[],
        )],
        LangKind::Ruby => &[("solargraph", "solargraph", &["stdio"])],
        LangKind::Php => &[("phpactor", "phpactor", &["language-server"])],
        LangKind::Html => &[(
            "html-ls",
            "vscode-html-language-server",
            &["--stdio"],
        )],
        LangKind::Css => &[(
            "css-ls",
            "vscode-css-language-server",
            &["--stdio"],
        )],
        LangKind::Json => &[(
            "json-ls",
            "vscode-json-language-server",
            &["--stdio"],
        )],
        LangKind::Toml => &[("taplo", "taplo", &["lsp", "stdio"])],
        LangKind::Yaml => &[(
            "yaml-ls",
            "yaml-language-server",
            &["--stdio"],
        )],
        LangKind::Markdown => &[("marksman", "marksman", &["server"])],
        LangKind::Shell => &[(
            "bash-ls",
            "bash-language-server",
            &["start"],
        )],
        LangKind::Sql => &[(
            "sql-ls",
            "sql-language-server",
            &["up", "--method", "stdio"],
        )],
        LangKind::Lua => &[(
            "lua-ls",
            "lua-language-server",
            &["--stdio"],
        )],
        LangKind::Laml | LangKind::Other => &[],
```

- [ ] **Step 6: Extend `symbols_for` kinds.** Old:

```rust
        LangKind::Rust => &["fn", "struct", "enum", "impl", "trait", "mod"],
```

New (keep the rest of the match, add arms before `LangKind::Other`):

```rust
        LangKind::Rust => &["fn", "struct", "enum", "impl", "trait", "mod"],
        LangKind::Python => &["def", "class"],
        LangKind::C => &["struct", "enum", "typedef"],
        LangKind::CSharp => &["class", "interface", "enum", "struct"],
        LangKind::Java => &["class", "interface", "enum"],
        LangKind::Swift => &["func", "class", "struct", "enum"],
        LangKind::Kotlin => &["fun", "class", "interface", "object"],
        LangKind::Ruby => &["def", "class", "module"],
        LangKind::Php => &["function", "class"],
        LangKind::Shell => &[],
        LangKind::Sql => &[],
        LangKind::Lua => &["function"],
```

And Markdown headers: at the top of the function body after the empty check, add:

```rust
    if lang == LangKind::Markdown {
        let mut out = Vec::new();
        for line in lines.iter().take(200) {
            let t = line.trim_start();
            if let Some(title) = t.strip_prefix('#') {
                let title = title.trim_start_matches('#').trim();
                if !title.is_empty() {
                    out.push(format!("# {title}"));
                }
                if out.len() >= 50 {
                    break;
                }
            }
        }
        return out;
    }
```

Also update the comment-skip line in `symbols_for` to use the shared helper. Old:

```rust
        if t.starts_with("//") || t.starts_with('~') || t.starts_with('#') {
            continue;
        }
```

New:

```rust
        if line_comment(lang).map(|m| t.starts_with(m)).unwrap_or(false) {
            continue;
        }
```

Careful: for `Html`, `<!--` mid-line (e.g. `<div><!-- x -->`) only skips lines STARTING with the marker — same limitation as before, honest.

- [ ] **Step 7: Render `lang_label` arms.** Old tail:

```rust
        keplr_lang::LangKind::Rust => String::from("rust"),
        keplr_lang::LangKind::Laml => String::from("laml"),
        keplr_lang::LangKind::Other => String::from("text"),
```

New:

```rust
        keplr_lang::LangKind::Rust => String::from("rust"),
        keplr_lang::LangKind::Laml => String::from("laml"),
        keplr_lang::LangKind::Python => String::from("python"),
        keplr_lang::LangKind::C => String::from("c"),
        keplr_lang::LangKind::CSharp => String::from("csharp"),
        keplr_lang::LangKind::Java => String::from("java"),
        keplr_lang::LangKind::Swift => String::from("swift"),
        keplr_lang::LangKind::Kotlin => String::from("kotlin"),
        keplr_lang::LangKind::Ruby => String::from("ruby"),
        keplr_lang::LangKind::Php => String::from("php"),
        keplr_lang::LangKind::Html => String::from("html"),
        keplr_lang::LangKind::Css => String::from("css"),
        keplr_lang::LangKind::Json => String::from("json"),
        keplr_lang::LangKind::Toml => String::from("toml"),
        keplr_lang::LangKind::Yaml => String::from("yaml"),
        keplr_lang::LangKind::Markdown => String::from("markdown"),
        keplr_lang::LangKind::Shell => String::from("shell"),
        keplr_lang::LangKind::Sql => String::from("sql"),
        keplr_lang::LangKind::Lua => String::from("lua"),
        keplr_lang::LangKind::Other => String::from("text"),
```

- [ ] **Step 8: Commit locally**

```bash
git add crates/keplr-lang/src/lib.rs crates/keplr-render/src/lib.rs
git commit -m "feat(lang): 25 languages with keywords comments lsp and symbols"
```

### Task GA-2: Install helpers + wasm gates + wasm CI job

**Files:**
- Modify: `crates/keplr-lang/src/lib.rs` (install fns + cfg gates)
- Modify: `crates/keplr-cli/src/main.rs` (`lsp-install` command)
- Modify: `.github/workflows/ci.yml` (wasm-check job)

**Interfaces:**
- Produces: `pub fn install_hint(name: &str) -> String`, `pub fn install_rust_analyzer(dest_dir: &Path) -> anyhow::Result<PathBuf>`, `keplr lsp-install <name> [--dir DIR]`

- [ ] **Step 1: Append install helpers** (end of lang lib):

```rust
pub fn install_hint(name: &str) -> String {
    match name {
        "rust-analyzer" => String::from("keplr lsp-install rust-analyzer"),
        "gopls" => String::from("go install golang.org/x/tools/gopls@latest"),
        "typescript-language-server" => {
            String::from("npm install -g typescript typescript-language-server")
        }
        "clangd" => String::from("pkg install clang  # or apt/brew install llvm"),
        "pyright" => String::from("npm install -g pyright"),
        "lua-language-server" | "lua-ls" => {
            String::from("pkg install lua-language-server  # or brew/apt")
        }
        "taplo" => String::from("cargo install taplo-cli --locked"),
        "marksman" => String::from("pkg install marksman  # or brew/apt"),
        "bash-language-server" | "bash-ls" => {
            String::from("npm install -g bash-language-server")
        }
        _ => format!("no managed recipe for `{name}`; install it and ensure it is on PATH"),
    }
}

fn rust_analyzer_asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "aarch64") => {
            Some("rust-analyzer-aarch64-unknown-linux-gnu.gz")
        }
        ("linux", "x86_64") => Some("rust-analyzer-x86_64-unknown-linux-gnu.gz"),
        ("macos", "aarch64") => Some("rust-analyzer-aarch64-apple-darwin.gz"),
        ("macos", "x86_64") => Some("rust-analyzer-x86_64-apple-darwin.gz"),
        _ => None,
    }
}

pub fn install_rust_analyzer(dest_dir: &Path) -> anyhow::Result<PathBuf> {
    let asset = rust_analyzer_asset()
        .ok_or_else(|| anyhow::anyhow!("no rust-analyzer build for this platform"))?;
    let url = format!(
        "https://github.com/rust-lang/rust-analyzer/releases/latest/download/{asset}"
    );
    std::fs::create_dir_all(dest_dir)?;
    let gz = dest_dir.join(asset);
    let out = std::process::Command::new("curl")
        .arg("-fsSL")
        .arg("-o")
        .arg(&gz)
        .arg(&url)
        .output()
        .map_err(|e| anyhow::anyhow!("curl missing or failed to spawn: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&gz);
        anyhow::bail!(
            "download failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let bin = dest_dir.join("rust-analyzer");
    let unzip = std::process::Command::new("gunzip")
        .arg("-f")
        .arg(&gz)
        .output()
        .map_err(|e| anyhow::anyhow!("gunzip missing or failed to spawn: {e}"))?;
    if !unzip.status.success() {
        anyhow::bail!(
            "gunzip failed: {}",
            String::from_utf8_lossy(&unzip.stderr)
        );
    }
    let downloaded = dest_dir.join(asset.trim_end_matches(".gz"));
    std::fs::rename(&downloaded, &bin)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(bin)
}
```

Note: `gunzip -f file.gz` deletes the archive and writes `file` next to it — hence the rename from the trimmed name. If asset naming ever changes, the rename errors honestly.

- [ ] **Step 2: wasm gates.** The process/env users in lang lib are `LamlProbe::binary`, `LamlProbe::check`, `command_present`, `spawn_lsp`, `install_rust_analyzer`, `laml_diagnostics`. Gate each:

`binary`: split into two cfg versions. Old:

```rust
    pub fn binary() -> Option<PathBuf> {
```

Replace with:

```rust
    #[cfg(not(target_arch = "wasm32"))]
    pub fn binary() -> Option<PathBuf> {
```

and after that function's closing brace add:

```rust
    #[cfg(target_arch = "wasm32")]
    pub fn binary() -> Option<PathBuf> {
        None
    }
```

Concretely: the current body is:

```rust
    pub fn binary() -> Option<PathBuf> {
        for candidate in [
            PathBuf::from("/data/data/com.termux/files/home/LAML/laml"),
            PathBuf::from("/data/data/com.termux/files/usr/bin/laml"),
            PathBuf::from("/usr/local/bin/laml"),
        ] {
            if candidate.exists() {
                return Some(candidate);
            }
        }
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths).find_map(|dir| {
                let p = dir.join("laml");
                p.exists().then_some(p)
            })
        })
    }
```

becomes:

```rust
    #[cfg(not(target_arch = "wasm32"))]
    pub fn binary() -> Option<PathBuf> {
        for candidate in [
            PathBuf::from("/data/data/com.termux/files/home/LAML/laml"),
            PathBuf::from("/data/data/com.termux/files/usr/bin/laml"),
            PathBuf::from("/usr/local/bin/laml"),
        ] {
            if candidate.exists() {
                return Some(candidate);
            }
        }
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths).find_map(|dir| {
                let p = dir.join("laml");
                p.exists().then_some(p)
            })
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn binary() -> Option<PathBuf> {
        None
    }
```

Same treatment for `check` (wasm version bails `"laml binary unavailable on wasm"`), `command_present` (wasm → `false`), `spawn_lsp` (wasm → bail `"language servers unavailable on wasm"`), `install_rust_analyzer` (wasm → bail `"installer unavailable on wasm"`), and `laml_diagnostics` — reroute through a helper so only one gate is needed. Current `laml_diagnostics` starts:

```rust
pub fn laml_diagnostics(source: &Path) -> Vec<Diagnostic> {
    let label = source.display().to_string();
    let Some(bin) = LamlProbe::binary() else {
```

That already degrades via `binary() == None` on wasm — no gate needed there. `check` is only called by nobody in-repo? `LamlProbe::check` callers: grep first; if only CLI/serve call diagnostics (not check), still gate `check` since `Command` won't compile on wasm. `install_rust_analyzer` uses `Command` + `std::env::consts` (consts is fine on wasm; Command is not) — gate the whole function with two versions (real + bail). `command_present` uses `std::env::var_os` (not available on wasm) — two versions.

Simplest uniform rule the implementer follows: any function body mentioning `std::process::Command` or `std::env::var_os` gets a `#[cfg(target_arch = "wasm32")]` twin that bails/returns-empty. Functions: `binary`, `check`, `command_present`, `spawn_lsp`, `install_rust_analyzer`. (`laml_diagnostics` needs no gate — it only calls `binary()`.)

- [ ] **Step 3: CLI command** (after the `Diagnostics` variant — read it; shape is `Diagnostics { file: PathBuf }`):

```rust
    LspInstall {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
```

Arm (after the `Diagnostics` arm):

```rust
        Cmd::LspInstall { name, dir } => {
            if name == "rust-analyzer" {
                let dest = dir.unwrap_or_else(|| cli.root.join(".keplr/bin"));
                let bin = keplr_lang::install_rust_analyzer(&dest)?;
                println!("installed to {}", bin.display());
            } else {
                let present = keplr_lang::command_present(&name);
                println!("present={present} hint: {}", keplr_lang::install_hint(&name));
            }
        }
```

- [ ] **Step 4: CI wasm job.** Append to `.github/workflows/ci.yml`:

```yaml
  wasm-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
          targets: wasm32-unknown-unknown
      - uses: Swatinem/rust-cache@v2
      - run: cargo check -p keplr-lang --target wasm32-unknown-unknown
```

(`dtolnay/rust-toolchain` supports `targets:` — real input.)

- [ ] **Step 5: Commit locally**

```bash
git add crates/keplr-lang/src/lib.rs crates/keplr-render/src/lib.rs crates/keplr-cli/src/main.rs .github/workflows/ci.yml
git commit -m "feat(lang): 25 languages plus installer and wasm slice"
```

(Task GA-1's commit and this one may land together or separately; keep messages accurate to content.)

---

## Part GB: Snippets + git manager

### Task GB-1: Snippet tables + expansion + endpoint

**Files:**
- Modify: `crates/keplr-lang/src/lib.rs` (append)
- Modify: `crates/keplr-serve/src/lib.rs` (`GET /snippets`)
- Modify: `crates/keplr-cli/src/main.rs` (`snippets` command)

**Interfaces:**
- Produces: `pub struct Snippet { pub prefix: String, pub body: String, pub description: String }`, `pub fn snippets_for(lang: LangKind) -> Vec<Snippet>`, `pub fn expand_snippet(lang: LangKind, prefix: &str) -> Option<(String, Option<usize>)>`, `GET /snippets?lang=&prefix=`, `keplr snippets <lang> [prefix]`

Marker language: `$0` marks the cursor; `${n:text}` inserts `text`. The returned offset is a char offset into the expanded text, or `None` when no `$0` is present.

- [ ] **Step 1: Append to lang lib** (end of file):

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Snippet {
    pub prefix: String,
    pub body: String,
    pub description: String,
}

fn snippet(lang: LangKind, prefix: &str, body: &str, description: &str) -> Snippet {
    let _ = lang;
    Snippet {
        prefix: prefix.to_string(),
        body: body.to_string(),
        description: description.to_string(),
    }
}

pub fn snippets_for(lang: LangKind) -> Vec<Snippet> {
    match lang {
        LangKind::Rust => vec![
            snippet(lang, "fn", "fn ${1:name}(${2:args}) {\n    $0\n}", "function"),
            snippet(lang, "struct", "struct ${1:Name} {\n    $0\n}", "struct"),
            snippet(lang, "match", "match ${1:value} {\n    Ok(v) => $0,\n    Err(e) => return Err(e.into()),\n}", "match result"),
            snippet(lang, "test", "#[test]\nfn ${1:name}() {\n    $0\n}", "unit test"),
        ],
        LangKind::TypeScript | LangKind::Tsx | LangKind::JavaScript => vec![
            snippet(lang, "fn", "function ${1:name}(${2:args}) {\n  $0\n}", "function"),
            snippet(lang, "af", "(${1:args}) => {\n  $0\n}", "arrow function"),
            snippet(lang, "imp", "import { $0 } from \"${1:mod}\";", "import"),
        ],
        LangKind::Python => vec![
            snippet(lang, "def", "def ${1:name}(${2:args}):\n    $0", "function"),
            snippet(lang, "class", "class ${1:Name}:\n    $0", "class"),
            snippet(lang, "ifmain", "if __name__ == \"__main__\":\n    $0", "main guard"),
        ],
        LangKind::Go => vec![
            snippet(lang, "func", "func ${1:name}(${2:args}) {\n\t$0\n}", "function"),
            snippet(lang, "struct", "type ${1:Name} struct {\n\t$0\n}", "struct"),
        ],
        LangKind::Laml => vec![
            snippet(lang, "serve", "serve ${1:port} {\n  $0\n}", "serve block"),
            snippet(lang, "on", "on ${1:event} {\n  $0\n}", "event handler"),
            snippet(lang, "send", "send(${1:room}, ${2:msg})", "send"),
        ],
        LangKind::Shell => vec![
            snippet(lang, "if", "if ${1:cond}; then\n  $0\nfi", "if block"),
            snippet(lang, "for", "for ${1:x} in ${2:list}; do\n  $0\ndone", "for loop"),
        ],
        _ => vec![
            snippet(lang, "todo", "// TODO: $0", "todo marker"),
        ],
    }
}

fn expand_markers(body: &str) -> (String, Option<usize>) {
    let mut out = String::new();
    let mut cursor = None;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                Some('0') => {
                    chars.next();
                    if cursor.is_none() {
                        cursor = Some(out.chars().count());
                    }
                }
                Some('{') => {
                    chars.next();
                    let mut num = String::new();
                    while let Some(d) = chars.peek() {
                        if d.is_ascii_digit() {
                            num.push(*d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if chars.peek() == Some(&':') {
                        chars.next();
                        let mut depth = 0;
                        for tc in chars.by_ref() {
                            if tc == '{' {
                                depth += 1;
                                out.push(tc);
                            } else if tc == '}' {
                                if depth == 0 {
                                    break;
                                }
                                depth -= 1;
                                out.push(tc);
                            } else {
                                out.push(tc);
                            }
                        }
                    } else if chars.peek() == Some(&'}') {
                        chars.next();
                    } else {
                        out.push('$');
                        out.push('{');
                        out.push_str(&num);
                    }
                }
                _ => {
                    out.push('$');
                }
            }
        } else {
            out.push(c);
        }
    }
    (out, cursor)
}

pub fn expand_snippet(lang: LangKind, prefix: &str) -> Option<(String, Option<usize>)> {
    snippets_for(lang)
        .into_iter()
        .find(|s| s.prefix == prefix)
        .map(|s| expand_markers(&s.body))
}
```

(The `let _ = lang;` in `snippet` keeps the helper uniform; the match arms carry the language.)

- [ ] **Step 2: Serve endpoint** (after `highlight`, before `pub async fn serve`):

```rust
async fn snippets(
    State(_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let lang = params
        .get("lang")
        .map(|l| keplr_lang::LangKind::from_path(Path::new(&format!("x.{l}"))))
        .unwrap_or(keplr_lang::LangKind::Other);
    let prefix = params.get("prefix").cloned().unwrap_or_default();
    if prefix.is_empty() {
        Json(serde_json::json!({ "snippets": keplr_lang::snippets_for(lang) }))
    } else {
        match keplr_lang::expand_snippet(lang, &prefix) {
            Some((text, cursor)) => {
                Json(serde_json::json!({ "text": text, "cursor": cursor }))
            }
            None => Json(serde_json::json!({ "error": "no such snippet" })),
        }
    }
}
```

Serve uses `Path` already (save handler added the import). Register `.route("/snippets", get(snippets))`.

`State(_state)` — unused state param can simply be omitted from the extractor list; axum allows handlers with just `Query`. Use `async fn snippets(Query(params): Query<HashMap<String, String>>)` — cleaner. (Implementer: omit State.)

- [ ] **Step 3: CLI command** (after the `Diagnostics` variant):

```rust
    Snippets {
        lang: String,
        prefix: Option<String>,
    },
```

Arm (after the `Diagnostics` arm — read it; it ends before `Cmd::Serve`):

```rust
        Cmd::Snippets { lang, prefix } => {
            let kind = keplr_lang::LangKind::from_path(Path::new(&format!("x.{lang}")));
            match prefix {
                Some(p) => match keplr_lang::expand_snippet(kind, &p) {
                    Some((text, cursor)) => {
                        println!("{text}");
                        if let Some(c) = cursor {
                            eprintln!("cursor={c}");
                        }
                    }
                    None => anyhow::bail!("no snippet `{p}` for {lang}"),
                },
                None => {
                    for s in keplr_lang::snippets_for(kind) {
                        println!("{} — {}", s.prefix, s.description);
                    }
                }
            }
        }
```

CLI needs `Path` import — check: main.rs imports `std::{collections::BTreeMap, path::PathBuf}`. Add `Path`: `use std::{collections::BTreeMap, path::{Path, PathBuf}};`.

- [ ] **Step 4: Commit locally**

```bash
git add crates/keplr-lang/src/lib.rs crates/keplr-serve/src/lib.rs crates/keplr-cli/src/main.rs
git commit -m "feat(snippets): tables plus expansion with cli and serve edges"
```

### Task GB-2: Git manager module + CLI + serve + TUI overlay data

**Files:**
- Create: `crates/keplr-core/src/git.rs`
- Modify: `crates/keplr-core/src/lib.rs` (`pub mod git;`)
- Modify: `crates/keplr-cli/src/main.rs` (`git` subcommands)
- Modify: `crates/keplr-serve/src/lib.rs` (`/git/*` routes)

**Interfaces:**
- Produces: `pub struct StatusEntry { pub path: String, pub staged: char, pub unstaged: char }`, `pub struct LogEntry { pub hash: String, pub author: String, pub date: String, pub message: String }`, `pub struct Branch { pub name: String, pub current: bool }`, `pub fn status(workdir: &Path) -> anyhow::Result<Vec<StatusEntry>>`, `pub fn log(workdir: &Path, limit: usize) -> anyhow::Result<Vec<LogEntry>>`, `pub fn branches(workdir: &Path) -> anyhow::Result<Vec<Branch>>`, `pub fn diff_stat(workdir: &Path) -> anyhow::Result<String>`, `pub fn commit(workdir: &Path, message: &str) -> anyhow::Result<String>`, `keplr git (status|log|branches|diff|commit)`, `GET /git/status /git/log /git/branches /git/diff`

- [ ] **Step 1: Create `crates/keplr-core/src/git.rs`:**

```rust
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StatusEntry {
    pub path: String,
    pub staged: char,
    pub unstaged: char,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Branch {
    pub name: String,
    pub current: bool,
}

fn git(workdir: &Path, args: &[&str]) -> anyhow::Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("git failed to spawn: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn xy(pair: &str) -> (char, char) {
    let mut c = pair.chars();
    (c.next().unwrap_or(' '), c.next().unwrap_or(' '))
}

pub fn status(workdir: &Path) -> anyhow::Result<Vec<StatusEntry>> {
    let out = git(workdir, &["status", "--porcelain=v1", "-z", "--untracked-files=normal"])?;
    let mut entries = Vec::new();
    for rec in out.split('\0') {
        if rec.len() < 4 {
            continue;
        }
        let (staged, unstaged) = xy(&rec[..2]);
        let mut path = rec[3..].to_string();
        if path.contains(" -> ") {
            path = path.split(" -> ").last().unwrap_or(&path).to_string();
        }
        entries.push(StatusEntry {
            path,
            staged,
            unstaged,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

pub fn log(workdir: &Path, limit: usize) -> anyhow::Result<Vec<LogEntry>> {
    let n = limit.clamp(1, 200).to_string();
    let out = git(
        workdir,
        &[
            "log",
            "--format=%H%x1f%an%x1f%ad%x1f%s",
            "--date=short",
            "-n",
            &n,
        ],
    )?;
    let mut entries = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split('\x1f').collect();
        if f.len() != 4 {
            continue;
        }
        entries.push(LogEntry {
            hash: f[0].to_string(),
            author: f[1].to_string(),
            date: f[2].to_string(),
            message: f[3].to_string(),
        });
    }
    Ok(entries)
}

pub fn branches(workdir: &Path) -> anyhow::Result<Vec<Branch>> {
    let out = git(workdir, &["branch", "--list"])?;
    let mut list = Vec::new();
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let (current, name) = match t.strip_prefix("* ") {
            Some(n) => (true, n),
            None => (false, t),
        };
        let name = match name.strip_prefix("+ ") {
            Some(n) => n,
            None => name,
        };
        list.push(Branch {
            name: name.to_string(),
            current,
        });
    }
    Ok(list)
}

pub fn diff_stat(workdir: &Path) -> anyhow::Result<String> {
    git(workdir, &["diff", "--stat"])
}

pub fn commit(workdir: &Path, message: &str) -> anyhow::Result<String> {
    git(workdir, &["add", "-A"])?;
    git(workdir, &["commit", "-m", message])
}
```

Porcelain `-z` renames arrive as `R  new\0old\0`; the `contains(" -> ")` guard plus NUL split keeps the common cases honest; quoted paths (`"a b"`) are returned as-is, documented behavior.

- [ ] **Step 2: Register the module.** In `crates/keplr-core/src/lib.rs` after the imports add:

```rust
pub mod git;
```

- [ ] **Step 3: CLI.** Variant (after `Snippets`):

```rust
    Git {
        #[command(subcommand)]
        cmd: GitCmd,
    },
```

Enum (before `enum Cmd` or after — place right above it):

```rust
#[derive(Subcommand)]
enum GitCmd {
    Status,
    Log {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Branches,
    Diff,
    Commit {
        #[arg(short, long)]
        message: String,
    },
}
```

Arm (after the `Snippets` arm):

```rust
        Cmd::Git { cmd } => match cmd {
            GitCmd::Status => {
                for e in keplr_core::git::status(&cli.root)? {
                    println!("{}{} {}", e.staged, e.unstaged, e.path);
                }
            }
            GitCmd::Log { limit } => {
                for e in keplr_core::git::log(&cli.root, limit)? {
                    println!("{} {} {} {}", &e.hash[..8.min(e.hash.len())], e.date, e.author, e.message);
                }
            }
            GitCmd::Branches => {
                for b in keplr_core::git::branches(&cli.root)? {
                    let mark = if b.current { "*" } else { " " };
                    println!("{mark} {}", b.name);
                }
            }
            GitCmd::Diff => {
                print!("{}", keplr_core::git::diff_stat(&cli.root)?);
            }
            GitCmd::Commit { message } => {
                print!("{}", keplr_core::git::commit(&cli.root, &message)?);
            }
        },
```

`&e.hash[..8.min(e.hash.len())]` — byte-slice a hex hash (ASCII, safe).

- [ ] **Step 4: Serve routes** (after the `snippets` handler):

```rust
async fn git_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    match keplr_core::git::status(&state.root) {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": format!("{e:#}") })),
    }
}

async fn git_log(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(20);
    match keplr_core::git::log(&state.root, limit) {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": format!("{e:#}") })),
    }
}

async fn git_branches(State(state): State<AppState>) -> Json<serde_json::Value> {
    match keplr_core::git::branches(&state.root) {
        Ok(entries) => Json(serde_json::json!({ "entries": entries })),
        Err(e) => Json(serde_json::json!({ "error": format!("{e:#}") })),
    }
}

async fn git_diff(State(state): State<AppState>) -> Json<serde_json::Value> {
    match keplr_core::git::diff_stat(&state.root) {
        Ok(stat) => Json(serde_json::json!({ "stat": stat })),
        Err(e) => Json(serde_json::json!({ "error": format!("{e:#}") })),
    }
}
```

Register:

```rust
        .route("/git/status", get(git_status))
        .route("/git/log", get(git_log))
        .route("/git/branches", get(git_branches))
        .route("/git/diff", get(git_diff))
```

- [ ] **Step 5: Commit locally**

```bash
git add crates/keplr-core/src/git.rs crates/keplr-core/src/lib.rs crates/keplr-cli/src/main.rs crates/keplr-serve/src/lib.rs
git commit -m "feat(git): status log branches diff commit with cli and serve edges"
```

---

## Part GC: TUI depth (highlight, tabs, motion, snippets, git)

### Task GC-1: Highlighted rendering + file tabs + git overlay + snippet expand

**Files:**
- Modify: `crates/keplr-cli/src/tui.rs` (render spans, TabState vec, G overlay, Tab-expand)

**Interfaces:**
- Consumes: `keplr_lang::{highlight, expand_snippet, LangKind}`, `keplr_core::git::status`
- Produces: same `edit_file` entry, deeper behavior

- [ ] **Step 1: Span-colored line printing.** Add a helper and a tabs row. Insert after `truncate_cells`:

```rust
fn colored_spans(lang: keplr_lang::LangKind, line: &str, max: usize) -> String {
    let text: String = line.chars().take(max).collect();
    let spans = keplr_lang::highlight(lang, &text);
    if spans.iter().all(|s| s.kind == keplr_lang::TokenKind::Other) {
        return text;
    }
    let mut out = String::new();
    for s in &spans {
        let piece: String = text.chars().skip(s.start).take(s.len).collect();
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
                out.push_str(&piece);
            }
        }
    }
    out
}
```

Span offsets are byte offsets into ASCII-safe boundaries; `text` here is the truncated-then-rehighlighted line so offsets always align (highlight runs on the exact string being sliced — char vs byte mismatch: `s.start`/`s.len` are BYTE offsets from `highlight`, but `chars().skip().take()` counts CHARS. For non-ASCII lines this mis-slices (still valid UTF-8 only if boundaries land on char edges — byte offsets into multibyte text used as char counts panic or garble). Fix honestly: slice by bytes with char-boundary floor:

```rust
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
```

Use `byte_slice(&text, s.start, s.len)` in `colored_spans`. (Implementer: use this version, not the chars().skip draft above.)

- [ ] **Step 2: Tabs.** Restructure `edit_file` state: replace the single `Doc`/cursor/top/dirty with:

```rust
struct TabState {
    path: PathBuf,
    label: String,
    lang: keplr_lang::LangKind,
    doc: Doc,
    cursor: (usize, usize),
    top: usize,
    dirty: bool,
}
```

`edit_file` opens the first tab from `full`, keeps `tabs: Vec<TabState>`, `active: usize`. Concretely, after the `ScreenGuard`, replace:

```rust
    let mut doc = Doc::load(&full);
    let mut cursor = (0usize, 0usize);
    let mut top = 0usize;
    let mut dirty = false;
```

with:

```rust
    let mut tabs = vec![TabState {
        label: file_label.clone(),
        lang: keplr_lang::LangKind::from_path(&full),
        doc: Doc::load(&full),
        cursor: (0, 0),
        top: 0,
        dirty: false,
        path: full.clone(),
    }];
    let mut active = 0usize;
```

Then every use of `doc`/`cursor`/`top`/`dirty`/`file_label`/`lang` in the loop body becomes `tabs[active].*` — this is a mechanical rewrite of the whole loop. To keep it reviewable, do it as: add the struct, add `fn cur(tabs: &mut Vec<TabState>, active: usize) -> &mut TabState` helper? Borrow rules make a helper awkward; instead index explicitly. The full rewritten `edit_file` is long — the implementer rewrites the function bodyOilowing these rules:
  - `doc` → `tabs[active].doc`, `cursor` → `tabs[active].cursor`, `top` → `tabs[active].top`, `dirty` → `tabs[active].dirty`, `file_label` → `tabs[active].label.clone()` (for draw), `lang` → `tabs[active].lang`.
  - Palette Enter: if the hit is already open, set `active` to it; else push a new `TabState` and activate it.
  - Draw gains a tabs row under the title: `name●` per tab, active reversed.
  - `Ctrl+]` → next tab, `Ctrl+[` → previous tab.
  - `G` (no modifiers, palette closed) toggles the git overlay, populated once per toggle from `keplr_core::git::status(&root)` (errors render as one status line, overlay shows the error text).
  - `Tab` key: take the word before the cursor (`[A-Za-z0-9_]+` ending at cursor); if `expand_snippet(lang, word)` hits, replace the word and place the cursor at the marker (or end); else insert two spaces (existing behavior).
  - Quit-dirty check considers ANY tab dirty.

Given the size, the implementer rewrites `tui.rs` `edit_file` wholesale following the existing structure — every key branch keeps its logic, only the state addressing changes, plus the five additions above. The draw function gains `tabs: &[(String, bool, bool)]` (label, active, dirty) and `git_open: bool` + `git_lines: &[String]` params... that re-trips `too_many_arguments`. Instead: draw takes `&Frame` (extended with `tabs: Vec<(String, bool, bool)>`, `git_open: bool`, `git_lines: Vec<String>`, `lang: LangKind` for span coloring) — extend the existing `Frame` struct. Span coloring needs the language: color each visible line with `colored_spans(lang, line, max)`.

Squiggle rows: after a line that has error diagnostics for this file? The TUI has no diagnostics wired — add: on open + on save, if file is `.lm`, run `keplr_lang::laml_diagnostics(&path)` and stash in the tab (`diags: Vec<Diagnostic>`); draw prints a red `~ msg` row under matching lines (cap 2 per line, like the ANSI painter). TabState gains `diags`.

This is the largest single edit of the plan; budget care over speed. No new files, no signature changes outside `tui.rs` (`edit_file` keeps its signature).

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-cli/src/tui.rs
git commit -m "feat(tui): highlight tabs squiggles snippets git overlay"
```

### Task GC-2: Motion (eased scroll, blink, pacing, reduced motion)

**Files:**
- Modify: `crates/keplr-cli/src/tui.rs` (frame clock, scroll easing, blink)

- [ ] **Step 1: Easing + clock.** Add:

```rust
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn reduced_motion(no_flag: bool) -> bool {
    no_flag
        || std::env::var("KEPLR_REDUCED_MOTION")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
}
```

Scroll state per tab: `top_f: f32` (render position) alongside `top: usize` (target). Each frame with `dt` seconds since last frame: if reduced motion, `top_f = top as f32`; else `top_f += (top as f32 - top_f) * (dt * 14.0).min(1.0)`, then ease the remainder by rendering `top_f.round()`. Simpler honest approach used here: critically-damped approach (no oscillation, no overshoot) — motion feels smooth without spring risk.

Blink: `last_key: Instant`, `blink_on: bool`; each frame, `blink_on = reduced || now - last_key < 530ms || (now.elapsed_ms / 530) % 2 == 0`... precise: visible when `now.duration_since(last_key) < Duration::from_millis(530)` (just typed) or the 1060ms cycle is in its first half. Hide cursor via `Hide` when blinking off (instead of MoveTo). Any key sets `last_key = now`.

Pacing: `poll_timeout = if animating_or_blink_due { 16ms } else { 200ms }` where animating = `(top_f - top).abs() > 0.5`.

- [ ] **Step 2: CLI flag.** `Edit` variant gains `#[arg(long)] no_animations: bool`, passed into `edit_file(root, file, no_animations)` — signature change is internal to the binary (update the arm + all `edit_file` call sites, which is just the arm).

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-cli/src/tui.rs crates/keplr-cli/src/main.rs
git commit -m "feat(tui): eased scroll blink pacing with reduced-motion respect"
```

---

## Part GD: Security hardening + supervised daemons + cached logs

### Task GD-1: Bind control + LAN refusal + rate limit + rotate

**Files:**
- Modify: `crates/keplr-serve/src/lib.rs` (AppState attempts, ConnectInfo, serve_full, middleware counting)
- Modify: `crates/keplr-cli/src/main.rs` (Serve `--bind/--allow-open-lan`, Token `--rotate`)

**Interfaces:**
- Produces: `pub async fn serve_full(root: PathBuf, port: u16, token: String, bind: &str, allow_open_lan: bool) -> anyhow::Result<()>`; `serve`/`serve_with_token` unchanged and delegate with `127.0.0.1`

- [ ] **Step 1: Serve changes.** Add imports:

```rust
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};
```

Check the current `use std::` block first (it has `collections::HashMap, path::{Path, PathBuf}`).

AppState gains attempts:

```rust
#[derive(Clone)]
struct AppState {
    root: PathBuf,
    token: String,
    attempts: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}
```

Update the two constructors (`serve_with_token` builds `AppState { root, token }`): both become `AppState { root, token, attempts: Arc::new(Mutex::new(HashMap::new())) }`. Find them: `serve_with_token` body has `let state = AppState { root, token };`.

Middleware: add `axum::extract::ConnectInfo` param and failure counting:

```rust
async fn require_token(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if state.token.is_empty() {
        return next.run(req).await;
    }
    if locked_out(&state, &addr) {
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({"error": "too many attempts"})),
        )
            .into_response();
    }
    ... existing header/query checks ...
    if header_ok || query_ok.unwrap_or(false) {
        next.run(req).await
    } else {
        record_fail(&state, &addr);
        (UNAUTHORIZED ...).into_response()
    }
}

fn locked_out(state: &AppState, addr: &SocketAddr) -> bool {
    let mut map = state.attempts.lock().unwrap_or_else(|e| e.into_inner());
    let key = addr.ip().to_string();
    if let Some((n, since)) = map.get(&key).cloned() {
        if since.elapsed().as_secs() > 60 {
            map.remove(&key);
            return false;
        }
        if n >= 10 {
            return true;
        }
    }
    false
}

fn record_fail(state: &AppState, addr: &SocketAddr) {
    let mut map = state.attempts.lock().unwrap_or_else(|e| e.into_inner());
    let key = addr.ip().to_string();
    let entry = map.entry(key).or_insert((0, Instant::now()));
    entry.0 += 1;
    if entry.0 == 1 {
        entry.1 = Instant::now();
    }
}
```

Needs `use axum::extract::ConnectInfo;` — extend the extract import.

`serve_full`: after `serve_with_token`, add:

```rust
pub async fn serve_full(
    root: PathBuf,
    port: u16,
    token: String,
    bind: &str,
    allow_open_lan: bool,
) -> anyhow::Result<()> {
    let loopback = bind == "127.0.0.1" || bind == "::1" || bind == "localhost";
    if !loopback && token.is_empty() && !allow_open_lan {
        anyhow::bail!("refusing to serve LAN without a token (set --token, KEPLR_TOKEN, or pass --allow-open-lan)");
    }
    ... same body as serve_with_token but binding format!("{bind}:{port}") ...
}
```

To avoid duplicating the whole router body, restructure: `serve_with_token` calls `serve_full(root, port, token, "127.0.0.1", true)`, and `serve_full` holds the real body (router + graceful shutdown + daemon kill from GD-2 — GD-2 edits this same function; order the work: GD-1 writes serve_full with plain `axum::serve(listener, app).await?;`, GD-2 upgrades it).

`into_make_service_with_connect_info::<SocketAddr>()` replaces `app` in the `axum::serve` call (required for `ConnectInfo` to resolve; without it every request 500s — verify this line exists after the edit).

- [ ] **Step 2: CLI.** `Serve` gains:

```rust
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        #[arg(long)]
        allow_open_lan: bool,
```

Arm calls `serve_full(cli.root, port, resolved, &bind, allow_open_lan)`. `Token` gains `#[arg(long)] rotate: bool`; arm: if rotate or save → write fresh token to `.keplr/token` (0600); print path; else print token. Concretely replace the Token arm:

```rust
        Cmd::Token { save, rotate } => {
            if rotate {
                let token = keplr_serve::new_token();
                let dir = cli.root.join(".keplr");
                std::fs::create_dir_all(&dir)?;
                let path = dir.join("token");
                std::fs::write(&path, format!("{token}\n"))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &path,
                        std::fs::Permissions::from_mode(0o600),
                    );
                }
                println!("rotated: {}", path.display());
            } else if save {
                ... existing save block ...
            } else {
                println!("{token}");  // existing: generates + prints
            }
        }
```

Restructure cleanly: generate once at top (`let token = new_token();`), then branch. (Implementer: write it that way.)

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-serve/src/lib.rs crates/keplr-cli/src/main.rs
git commit -m "feat(security): bind control lan refusal rate limit rotate"
```

### Task GD-2: Daemon supervision + cached log endpoint

**Files:**
- Modify: `crates/keplr-build/src/lib.rs` (`daemon` flag, `output_tail`)
- Modify: `crates/keplr-serve/src/lib.rs` (spawn/supervise/routes/shutdown)

**Interfaces:**
- Produces: `TaskDef.daemon`, `JournalEntry.output_tail`, `GET /daemons`, `POST /daemons/restart`, `GET /tasks/log?name=`

- [ ] **Step 1: Build edits.** `TaskDef` + `RawTask` gain `#[serde(default)] pub daemon: bool` (wire through `load_tasks` like the other fields). `JournalEntry` gains `#[serde(default)] pub output_tail: String`. Add helper:

```rust
fn tail_2k(s: &str) -> String {
    if s.len() <= 2048 {
        return s.to_string();
    }
    s.chars()
        .rev()
        .take(2048)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
```

Set `output_tail: tail_2k(out-or-text)` at all four success constructions (`run_ordered` ok, parallel ok, settled ok ×2 — find each `JournalEntry { hash` site; failed/cancelled/skipped entries get `output_tail: String::new()` — there are no JournalEntry constructions for those paths (skipped/failed never touch the journal), so only the four success sites change.

- [ ] **Step 2: Serve supervision.** State gains:

```rust
#[derive(Clone)]
struct Supervised {
    cmd: String,
    started: std::time::SystemTime,
    pid: Option<u32>,
}
```

Storing `Child` across await points needs shared ownership: `Arc<Mutex<HashMap<String, (Supervised, std::process::Child)>>>`. Add to `AppState` as `daemons: Arc<Mutex<HashMap<String, (Supervised, std::process::Child)>>>`.

Spawn helper (free function):

```rust
fn spawn_daemon(
    root: &Path,
    def: &keplr_build::TaskDef,
) -> anyhow::Result<(Supervised, std::process::Child)> {
    let dir = root.join(".keplr/logs");
    std::fs::create_dir_all(&dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{}.log", def.name)))?;
    let err_log = log.try_clone()?;
    let cwd = def.cwd.as_ref().map(Path::new).unwrap_or(root);
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(&def.cmd)
        .current_dir(cwd)
        .stdout(std::process::Stdio::from(log))
        .stderr(std::process::Stdio::from(err_log))
        .spawn()?;
    let pid = child.id();
    Ok((
        Supervised {
            cmd: def.cmd.clone(),
            started: std::time::SystemTime::now(),
            pid,
        },
        child,
    ))
}
```

`child.id()` returns `Option<u32>` on stable? `Child::id` returns `u32` (panics pre-exec?) — stable signature: `pub fn id(&self) -> u32`. Use `Some(child.id())`. (If CI says otherwise, adjust.)

In `serve_full` after building `state`: load `keplr.json` (ignore errors), spawn every `daemon: true` task, log failures to stderr, insert into the map. Replace the tail:

```rust
    let listener = tokio::net::TcpListener::bind(format!("{bind}:{port}")).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    let mut daemons = state.daemons.lock().unwrap_or_else(|e| e.into_inner());
    for (name, (_, child)) in daemons.iter_mut() {
        let _ = child.kill();
        eprintln!("keplr: stopped daemon {name}");
    }
    Ok(())
```

`tokio::signal` needs the `signal` feature — `tokio = { version, features = ["full"] }` includes it. Good.

Routes:

```rust
async fn daemons(State(state): State<AppState>) -> Json<serde_json::Value> {
    let map = state.daemons.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = map
        .iter()
        .map(|(name, (s, _))| {
            serde_json::json!({ "name": name, "cmd": s.cmd, "pid": s.pid })
        })
        .collect();
    Json(serde_json::json!({ "daemons": list }))
}

#[derive(serde::Deserialize)]
struct DaemonReq {
    name: String,
}

async fn daemon_restart(
    State(state): State<AppState>,
    Json(req): Json<DaemonReq>,
) -> Json<serde_json::Value> {
    let path = state.root.join("keplr.json");
    let tasks = match keplr_build::load_tasks(&path) {
        Ok(m) => m,
        Err(e) => return Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    };
    let def = match tasks.get(&req.name) {
        Some(d) => d.clone(),
        None => return Json(serde_json::json!({"ok": false, "error": "unknown task"})),
    };
    let mut map = state.daemons.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, child)) = map.get_mut(&req.name) {
        let _ = child.kill();
    }
    match spawn_daemon(&state.root, &def) {
        Ok((sup, child)) => {
            map.insert(req.name.clone(), (sup, child));
            Json(serde_json::json!({"ok": true, "name": req.name}))
        }
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{e:#}")})),
    }
}

async fn task_log(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let name = params.get("name").cloned().unwrap_or_default();
    let journal = keplr_build::load_journal(&state.root);
    match journal.get(&name) {
        Some(e) => Json(serde_json::json!({
            "name": name,
            "hash": e.hash,
            "output_tail": e.output_tail,
        })),
        None => Json(serde_json::json!({ "error": "no cached log" })),
    }
}
```

Register `.route("/daemons", get(daemons))`, `.route("/daemons/restart", post(daemon_restart))`, `.route("/tasks/log", get(task_log))`.

`std::process::Child` in state across threads: `Child: Send` yes. The map type must be named for the struct field:

```rust
type DaemonMap = Arc<Mutex<HashMap<String, (Supervised, std::process::Child)>>>;
```

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-build/src/lib.rs crates/keplr-serve/src/lib.rs crates/keplr-cli/src/main.rs
git commit -m "feat(serve): supervised daemons plus cached task logs"
```

(Note: CLI `main.rs` is touched only if the bind/rotate edits land here — they belong to GD-1; keep commits accurate.)

---

## Part GE: Fonts, GPU atlas, WASM slice, verify

### Task GE-1: Font stack + `keplr fonts` + vendored JetBrains Mono

**Files:**
- Binary: `assets/fonts/JetBrainsMono-Regular.ttf`, `assets/fonts/JetBrainsMono-Bold.ttf` (already fetched, OFL)
- Modify: `crates/keplr-render/src/lib.rs` (`font_stack`, `discover_font`)
- Modify: `crates/keplr-cli/src/main.rs` (`fonts` command)
- Modify: `README.md` (font decision note)

- [ ] **Step 1: Append to render lib** (end of file):

```rust
pub fn font_stack() -> Vec<PathBuf> {
    let mut stack = Vec::new();
    if let Ok(f) = std::env::var("KEPLR_FONT") {
        if !f.trim().is_empty() {
            stack.push(PathBuf::from(f.trim()));
        }
    }
    stack.push(PathBuf::from("assets/fonts/JetBrainsMono-Regular.ttf"));
    stack.push(PathBuf::from("assets/fonts/JetBrainsMono-Bold.ttf"));
    for home in [dirs_home(), PathBuf::from("/data/data/com.termux/files/home")] {
        if home.as_os_str().is_empty() {
            continue;
        }
        stack.push(home.join(".keplr/fonts/JetBrainsMono-Regular.ttf"));
        stack.push(home.join(".fonts/JetBrainsMono-Regular.ttf"));
    }
    for sys in [
        "/Applications/Xcode.app/Contents/SharedSupport/Fonts/SFMono-Regular.otf",
        "/Library/Fonts/SFMono-Regular.otf",
        "/System/Library/Fonts/SFMono-Regular.otf",
        "/System/Library/Fonts/SFNSMono.ttf",
    ] {
        stack.push(PathBuf::from(sys));
    }
    for dir in [
        "/usr/share/fonts",
        "/usr/local/share/fonts",
        "/system/fonts",
    ] {
        stack.push(dir.join("JetBrainsMono-Regular.ttf"));
        stack.push(dir.join("DejaVuSansMono.ttf"));
        stack.push(dir.join("DroidSansMono.ttf"));
    }
    stack.push(PathBuf::from(
        "/system/fonts/DroidSansMono.ttf",
    ));
    stack
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn discover_font() -> Option<PathBuf> {
    font_stack().into_iter().find(|p| p.is_file())
}
```

SF Mono entries sit below the vendored/open fonts (license-safe order: open fonts win when present; SF Mono is used only where Apple put it). Hmm — user asked to "mainly choose" the Xcode font. Order decision: Apple's license permits USE on Apple systems, and preferring a present SF Mono on macOS honors "Xcode's font" where legal, while vendored JetBrains Mono guarantees the same metrics everywhere else. But the current order puts vendored first — on macOS dev machines that hides SF Mono. Fix the order: `KEPLR_FONT` override first, then Apple SF Mono paths, then vendored JetBrains Mono, then user/system scans. (Implementer: write it in that order — override, SF Mono ×4, vendored ×2, home scans, system scans.)

Revised order:

```rust
pub fn font_stack() -> Vec<PathBuf> {
    let mut stack = Vec::new();
    if let Ok(f) = std::env::var("KEPLR_FONT") {
        if !f.trim().is_empty() {
            stack.push(PathBuf::from(f.trim()));
        }
    }
    for sys in [
        "/Applications/Xcode.app/Contents/SharedSupport/Fonts/SFMono-Regular.otf",
        "/Library/Fonts/SFMono-Regular.otf",
        "/System/Library/Fonts/SFMono-Regular.otf",
        "/System/Library/Fonts/SFNSMono.ttf",
    ] {
        stack.push(PathBuf::from(sys));
    }
    stack.push(PathBuf::from("assets/fonts/JetBrainsMono-Regular.ttf"));
    stack.push(PathBuf::from("assets/fonts/JetBrainsMono-Bold.ttf"));
    let home = dirs_home();
    if !home.as_os_str().is_empty() {
        stack.push(home.join(".keplr/fonts/JetBrainsMono-Regular.ttf"));
        stack.push(home.join(".fonts/JetBrainsMono-Regular.ttf"));
    }
    stack.push(PathBuf::from(
        "/data/data/com.termux/files/home/.fonts/IosevkaTermNerdFontMono-Regular.ttf",
    ));
    for dir in [
        "/usr/share/fonts",
        "/usr/local/share/fonts",
        "/system/fonts",
    ] {
        let dir = PathBuf::from(dir);
        stack.push(dir.join("JetBrainsMono-Regular.ttf"));
        stack.push(dir.join("DejaVuSansMono.ttf"));
        stack.push(dir.join("DroidSansMono.ttf"));
    }
    stack
}
```

(Use this revised version, not the first draft.)

- [ ] **Step 2: CLI command** (after the `Token` variant — read it; `Token { save: bool }` becomes `Token { save, rotate }` in GD-1; add after that whole variant):

```rust
    Fonts,
```

Arm (after the `Token` arm):

```rust
        Cmd::Fonts => {
            let mut any = false;
            for p in keplr_render::font_stack() {
                let present = p.is_file();
                any = any || present;
                let mark = if present { "ok" } else { "--" };
                println!("{mark} {}", p.display());
            }
            if !any {
                println!("no monospace font found; set KEPLR_FONT=/path/to/font.ttf");
            }
        }
```

- [ ] **Step 3: README font note** (append near the Plan F section or a Fonts line in the header area — put after the Plan G link line... Plan G link line is added in GE-3; add the note there too):

```markdown
Fonts: SF Mono when macOS/Xcode provides it (Apple license, never vendored), else vendored JetBrains Mono OFL (`assets/fonts/`), else system monos. Override with `KEPLR_FONT`. `keplr fonts` shows the resolved stack.
```

- [ ] **Step 4: Commit locally (code only; binaries commit in GE-3 with everything)**

Actually commit here to keep history clean:

```bash
git add crates/keplr-render/src/lib.rs crates/keplr-cli/src/main.rs
git commit -m "feat(fonts): licensed stack preferring sf mono with discovery"
```

### Task GE-2: GPU glyph atlas + texture upload

**Files:**
- Modify: `Cargo.toml` (`ab_glyph = "0.2"` in workspace deps)
- Modify: `crates/keplr-render/Cargo.toml` (optional dep in `gpu` feature)
- Modify: `crates/keplr-render/src/gpu.rs` (atlas + upload)

- [ ] **Step 1: Deps.** Workspace append `ab_glyph = "0.2"`. Render Cargo:

```toml
[features]
gpu = ["dep:winit", "dep:wgpu", "dep:pollster", "dep:ab_glyph"]
```

and

```toml
ab_glyph = { workspace = true, optional = true }
```

- [ ] **Step 2: Atlas code** (append to `gpu.rs`):

```rust
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct GlyphSpot {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub advance: f32,
    pub bx: f32,
    pub by: f32,
}

pub struct GlyphAtlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub glyphs: HashMap<(char, u32), GlyphSpot>,
}

pub fn build_atlas(font_bytes: &[u8], px: f32) -> anyhow::Result<GlyphAtlas> {
    let font = FontRef::try_from_slice(font_bytes)
        .map_err(|e| anyhow::anyhow!("bad font: {e:?}"))?;
    let scaled = font.as_scaled(PxScale::from(px));
    let ascent = scaled.ascent();
    let mut atlas = GlyphAtlas {
        width: 512,
        height: 512,
        pixels: vec![0u8; 512 * 512],
        glyphs: HashMap::new(),
    };
    let mut pen_x = 0u32;
    let mut pen_y = 0u32;
    let mut row_h = 0u32;
    let id = px as u32;
    for b in 32u8..127u8 {
        let c = b as char;
        let glyph = scaled.scaled_glyph(c);
        let outlined = match scaled.outline_glyph(glyph) {
            Some(o) => o,
            None => {
                let adv = scaled.h_advance(glyph.id);
                atlas.glyphs.insert(
                    (c, id),
                    GlyphSpot {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0,
                        advance: adv,
                        bx: 0.0,
                        by: 0.0,
                    },
                );
                continue;
            }
        };
        let bounds = outlined.px_bounds();
        let w = bounds.width() as u32;
        let h = bounds.height() as u32;
        if w == 0 || h == 0 {
            continue;
        }
        if pen_x + w > atlas.width {
            pen_x = 0;
            pen_y += row_h;
            row_h = 0;
        }
        if pen_y + h > atlas.height {
            anyhow::bail!("atlas overflow at {px}px");
        }
        outlined.draw(|x, y, v| {
            let gx = pen_x + x;
            let gy = pen_y + y;
            atlas.pixels[(gy * atlas.width + gx) as usize] =
                (v.clamp(0.0, 1.0) * 255.0) as u8;
        });
        let adv = scaled.h_advance(outlined.glyph().id);
        atlas.glyphs.insert(
            (c, id),
            GlyphSpot {
                x: pen_x,
                y: pen_y,
                w,
                h,
                advance: adv,
                bx: bounds.min.x - 0.0,
                by: ascent - bounds.min.y,
            },
        );
        pen_x += w + 1;
        row_h = row_h.max(h + 1);
    }
    Ok(atlas)
}

pub fn upload_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &GlyphAtlas,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("keplr-glyphs"),
        size: wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas.pixels,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width),
            rows_per_image: Some(atlas.height),
        },
        wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
    );
    texture
}
```

`outlined.glyph().id` — `OutlinedGlyph::glyph()` returns `&Glyph`; `.id` field access. If CI disagrees, adjust from the error. `bounds.min.x` — `Rect { min: Point, max: Point }`, `width()`/`height()` methods exist on Rect. `bx: bounds.min.x - 0.0` is silly — write `bx: bounds.min.x`. (Implementer: drop the `- 0.0`.)

`by: ascent - bounds.min.y` — baseline-relative top offset. Fine.

`use std::collections::HashMap;` — gpu.rs currently imports none of std collections; add.

- [ ] **Step 3: Commit locally**

```bash
git add Cargo.toml crates/keplr-render/Cargo.toml crates/keplr-render/src/gpu.rs
git commit -m "feat(gpu): glyph atlas plus texture upload"
```

### Task GE-3: WASM slice + push + verify

**Files:**
- Modify: `crates/keplr-lang/src/lib.rs` (cfg twins for process/env fns)
- Modify: `.github/workflows/ci.yml` (wasm-check job)
- Modify: `README.md` (Plan G link + usage + font note)
- Binary: `assets/fonts/*.ttf`

- [ ] **Step 1: cfg twins.** Rule: every function body mentioning `std::process::Command` or `std::env::var_os` gets a wasm twin. Functions: `LamlProbe::binary`, `LamlProbe::check`, `command_present`, `spawn_lsp`, `install_rust_analyzer`. Pattern per function (example `command_present`):

```rust
#[cfg(not(target_arch = "wasm32"))]
pub fn command_present(cmd: &str) -> bool {
    ... existing ...
}

#[cfg(target_arch = "wasm32")]
pub fn command_present(_cmd: &str) -> bool {
    false
}
```

Wasm twins: `binary → None`, `check → bail "laml binary unavailable on wasm"`, `command_present → false`, `install_rust_analyzer → bail "installer unavailable on wasm"`. `spawn_lsp` is gated whole (`#[cfg(not(target_arch = "wasm32"))]`, no twin — `std::process::Child` does not resolve on wasm).

`laml_diagnostics` needs no gate (degrades through `binary()`).

- [ ] **Step 2: CI job.** Append:

```yaml
  wasm-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
          targets: wasm32-unknown-unknown
      - uses: Swatinem/rust-cache@v2
      - run: cargo check -p keplr-lang --target wasm32-unknown-unknown
```

- [ ] **Step 3: README.** Add `Plan G all-in: \`docs/superpowers/plans/2026-09-18-keplr-plan-g-all-in.md\`` next to the Plan F link, the font note from GE-1, and append:

```markdown
## Use (Plan G all-in, production)

cargo run -p keplr-cli -- --root . fonts
cargo run -p keplr-cli -- --root . snippets rs
cargo run -p keplr-cli -- --root . git status
cargo run -p keplr-cli -- --root . git log --limit 5
cargo run -p keplr-cli -- --root . lsp-install rust-analyzer --dir ~/.keplr/bin
cargo run -p keplr-cli -- --root . edit src/main.rs
cargo run -p keplr-cli -- --root . serve --bind 127.0.0.1 --port 7137
cargo run -p keplr-cli -- --root . token --rotate
curl '127.0.0.1:7137/git/status'
curl '127.0.0.1:7137/snippets?lang=rs'
curl '127.0.0.1:7137/daemons'
curl '127.0.0.1:7137/tasks/log?name=lint'
```

- [ ] **Step 4: Commit (including fonts), push, watch**

```bash
git add crates/keplr-lang/src/lib.rs .github/workflows/ci.yml README.md assets/fonts/
git commit -m "feat(wasm): lang slice with ci check plus docs and fonts"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: three jobs green. On failure: `gh run view <id> --log-failed`, fix, push again.

---

## Self-review (run before handoff)

1. Spec coverage: §7 all-major-languages yes (25 kinds, keywords, comments, LSP data, symbols, installer); snippets yes (tables + expand + TUI Tab + endpoint); git manager yes (status/log/branches/diff/commit + CLI + serve + TUI overlay); §4 typing UX yes (highlight, tabs, squiggles, eased scroll, blink, reduced motion); §5 hub hardening yes (bind, LAN refusal, rate limit, rotate); daemons yes (serve-supervised + routes); §6 replay yes (`output_tail` + `/tasks/log`); §2 font stack yes (licensed order, vendored JetBrains Mono, discovery, atlas); WASM slice yes (lang compiles for wasm32, CI-verified).
2. Placeholder scan: no TBD/TODO/placeholder/unimplemented; missing servers/binaries report hints; GPU-less desktop falls back; empty token means documented open localhost mode.
3. Type consistency: `line_comment`, `install_hint`, `install_rust_analyzer`, `Snippet/snippets_for/expand_snippet`, `git::{StatusEntry, LogEntry, Branch, status, log, branches, diff_stat, commit}`, `font_stack/discover_font`, `GlyphAtlas/GlyphSpot/build_atlas/upload_atlas`, `serve_full`, `output_tail`, `daemon` spelled identically at every definition and call site.
