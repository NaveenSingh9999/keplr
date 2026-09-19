use std::path::{Path, PathBuf};

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

impl LangKind {
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "ts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "js" | "jsx" => Self::JavaScript,
            "cpp" | "cc" | "cxx" | "h" | "hpp" => Self::Cpp,
            "go" => Self::Go,
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
        }
    }
}

pub struct LamlProbe;

impl LamlProbe {
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

    #[cfg(not(target_arch = "wasm32"))]
    pub fn check(source: &Path) -> anyhow::Result<String> {
        let bin = Self::binary().ok_or_else(|| anyhow::anyhow!("laml binary not found"))?;
        let output = std::process::Command::new(bin).arg("check").arg(source).output()?;
        let mut s = String::from_utf8_lossy(&output.stdout).to_string();
        s.push_str(&String::from_utf8_lossy(&output.stderr));
        if !output.status.success() {
            anyhow::bail!("laml check failed: {s}");
        }
        Ok(s)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn check(_source: &Path) -> anyhow::Result<String> {
        anyhow::bail!("laml binary unavailable on wasm")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TokenKind {
    Keyword,
    Str,
    Comment,
    Number,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub len: usize,
    pub kind: TokenKind,
}

fn keywords(lang: LangKind) -> &'static [&'static str] {
    match lang {
        LangKind::Rust => &[
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "use",
            "mod", "crate", "self", "Self", "return", "if", "else", "match",
            "for", "while", "loop", "in", "where", "const", "static", "ref",
            "move", "async", "await", "dyn", "unsafe", "extern", "as", "break",
            "continue", "true", "false", "Some", "None", "Ok", "Err", "type",
        ],
        LangKind::TypeScript | LangKind::Tsx | LangKind::JavaScript => &[
            "function", "const", "let", "var", "return", "if", "else", "for",
            "while", "import", "export", "from", "class", "extends", "new",
            "typeof", "interface", "type", "enum", "async", "await", "try",
            "catch", "throw", "switch", "case", "break", "continue", "this",
            "true", "false", "null", "undefined",
        ],
        LangKind::Cpp => &[
            "int", "float", "double", "char", "bool", "void", "class",
            "struct", "public", "private", "protected", "virtual", "override",
            "final", "template", "typename", "namespace", "using", "return",
            "if", "else", "for", "while", "new", "delete", "const", "static",
            "auto", "true", "false", "nullptr", "include",
        ],
        LangKind::Go => &[
            "func", "var", "const", "type", "struct", "interface", "map",
            "chan", "go", "select", "return", "if", "else", "for", "range",
            "switch", "case", "break", "continue", "package", "import",
            "true", "false", "nil",
        ],
        LangKind::Laml => &[
            "serve", "on", "send", "broadcast", "joinRoom", "members",
            "async", "waitFor", "closc", "sort", "pop", "join", "upper",
            "lower", "keys", "has", "assert", "jsonParse", "jsonStringify",
            "setTimeout", "return", "if", "else", "for", "true", "false",
            "null",
        ],
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
        LangKind::Other => &[],
    }
}

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

fn push_other(spans: &mut Vec<Span>, other_start: &mut Option<usize>, end: usize) {
    if let Some(s) = other_start.take() {
        if end > s {
            spans.push(Span {
                start: s,
                len: end - s,
                kind: TokenKind::Other,
            });
        }
    }
}

pub fn highlight(lang: LangKind, line: &str) -> Vec<Span> {
    let bytes = line.as_bytes();
    let mut spans: Vec<Span> = Vec::new();
    let mut other_start: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
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
        if b == b'"' {
            push_other(&mut spans, &mut other_start, i);
            let mut j = i + 1;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'"' {
                    j += 1;
                    break;
                }
                j += 1;
            }
            spans.push(Span {
                start: i,
                len: j - i,
                kind: TokenKind::Str,
            });
            i = j;
            continue;
        }
        if b.is_ascii_digit() {
            push_other(&mut spans, &mut other_start, i);
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'.')
            {
                j += 1;
            }
            spans.push(Span {
                start: i,
                len: j - i,
                kind: TokenKind::Number,
            });
            i = j;
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            push_other(&mut spans, &mut other_start, i);
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
            {
                j += 1;
            }
            let word = &line[i..j];
            let kind = if keywords(lang).contains(&word) {
                TokenKind::Keyword
            } else {
                TokenKind::Other
            };
            spans.push(Span {
                start: i,
                len: j - i,
                kind,
            });
            i = j;
            continue;
        }
        if other_start.is_none() {
            other_start = Some(i);
        }
        i += 1;
    }
    push_other(&mut spans, &mut other_start, bytes.len());
    spans
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub path: String,
    pub line: u64,
    pub col: u64,
    pub message: String,
    pub severity: String,
}

fn hint_diagnostic(source: &Path, message: &str) -> Diagnostic {
    Diagnostic {
        path: source.display().to_string(),
        line: 1,
        col: 1,
        message: message.to_string(),
        severity: String::from("hint"),
    }
}

fn parse_diag_line(raw: &str, fallback_path: &str) -> Option<Diagnostic> {
    let parts: Vec<&str> = raw.splitn(4, ':').collect();
    if parts.len() < 3 {
        return None;
    }
    let line: u64 = parts[1].trim().parse().ok()?;
    if line == 0 {
        return None;
    }
    let (col, message) = if parts.len() == 4 {
        match parts[2].trim().parse::<u64>() {
            Ok(c) if c > 0 => (c, parts[3].trim().to_string()),
            _ => (1, format!("{}: {}", parts[2].trim(), parts[3].trim())),
        }
    } else {
        (1, parts[2].trim().to_string())
    };
    if message.is_empty() {
        return None;
    }
    let path = if parts[0].trim().is_empty() {
        fallback_path.to_string()
    } else {
        parts[0].trim().to_string()
    };
    Some(Diagnostic {
        path,
        line,
        col,
        message,
        severity: String::from("error"),
    })
}

pub fn laml_diagnostics(source: &Path) -> Vec<Diagnostic> {
    let label = source.display().to_string();
    let Some(bin) = LamlProbe::binary() else {
        return vec![hint_diagnostic(
            source,
            "laml binary not found on PATH; install it for check/run diagnostics",
        )];
    };
    let out = std::process::Command::new(&bin)
        .arg("check")
        .arg(source)
        .output();
    let Ok(out) = out else {
        return vec![hint_diagnostic(source, "laml binary could not be spawned")];
    };
    if out.status.success() {
        return Vec::new();
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut diags: Vec<Diagnostic> = text
        .lines()
        .filter_map(|raw| parse_diag_line(raw, &label))
        .collect();
    if diags.is_empty() {
        diags.push(Diagnostic {
            path: label,
            line: 1,
            col: 1,
            message: text.chars().take(500).collect(),
            severity: String::from("error"),
        });
    }
    diags
}

const LAML_KEYWORDS: &[&str] = &[
    "serve",
    "on",
    "send",
    "broadcast",
    "joinRoom",
    "members",
    "async",
    "waitFor",
    "closc",
    "sort",
    "pop",
    "join",
    "upper",
    "lower",
    "keys",
    "has",
    "assert",
    "jsonParse",
    "jsonStringify",
    "setTimeout",
    "return",
    "if",
    "else",
    "for",
    "true",
    "false",
    "null",
];

pub fn laml_completions(prefix: &str) -> Vec<String> {
    LAML_KEYWORDS
        .iter()
        .filter(|k| k.starts_with(prefix))
        .map(|k| k.to_string())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LspServer {
    pub name: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub present: bool,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn command_present(cmd: &str) -> bool {
    if cmd.contains('/') {
        return Path::new(cmd).exists();
    }
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join(cmd).exists())
        })
        .unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
pub fn command_present(_cmd: &str) -> bool {
    false
}

pub fn lsp_servers(lang: LangKind) -> Vec<LspServer> {
    let defs: &[(&str, &str, &[&str])] = match lang {
        LangKind::TypeScript | LangKind::Tsx | LangKind::JavaScript => &[(
            "typescript-language-server",
            "typescript-language-server",
            &["--stdio"],
        )],
        LangKind::Cpp => &[("clangd", "clangd", &["--background-index"])],
        LangKind::Go => &[("gopls", "gopls", &["serve"])],
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
    };
    defs.iter()
        .map(|(name, cmd, args)| LspServer {
            name: name.to_string(),
            cmd: cmd.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            present: command_present(cmd),
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_lsp(server: &LspServer) -> anyhow::Result<std::process::Child> {
    if !server.present {
        anyhow::bail!(
            "language server `{}` not found ({}); install it first",
            server.name,
            server.cmd
        );
    }
    std::process::Command::new(&server.cmd)
        .args(&server.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to start `{}`: {}", server.cmd, e))
}

#[cfg(target_arch = "wasm32")]
pub fn spawn_lsp(_server: &LspServer) -> anyhow::Result<()> {
    anyhow::bail!("language servers unavailable on wasm")
}

pub fn symbols_for(lang: LangKind, lines: &[String]) -> Vec<String> {
    symbols_detailed(lang, lines)
        .into_iter()
        .map(|s| format!("{} {}", s.kind, s.name))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SymbolInfo {
    pub line: u64,
    pub kind: String,
    pub name: String,
}

pub fn symbols_detailed(lang: LangKind, lines: &[String]) -> Vec<SymbolInfo> {
    let kinds: &[&str] = match lang {
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
        LangKind::Html
        | LangKind::Css
        | LangKind::Json
        | LangKind::Toml
        | LangKind::Yaml
        | LangKind::Markdown => &[],
        LangKind::TypeScript | LangKind::Tsx | LangKind::JavaScript => {
            &["function", "class", "interface"]
        }
        LangKind::Cpp => &["class", "struct"],
        LangKind::Go => &["func", "type"],
        LangKind::Laml => &["serve", "on"],
        LangKind::Other => &[],
    };
    if kinds.is_empty() {
        return Vec::new();
    }
    if lang == LangKind::Markdown {
        let mut out = Vec::new();
        for (idx, line) in lines.iter().take(200).enumerate() {
            let t = line.trim_start();
            if let Some(title) = t.strip_prefix('#') {
                let title = title.trim_start_matches('#').trim();
                if !title.is_empty() {
                    out.push(SymbolInfo {
                        line: (idx + 1) as u64,
                        kind: String::from("#"),
                        name: title.to_string(),
                    });
                }
                if out.len() >= 50 {
                    break;
                }
            }
        }
        return out;
    }
    let mut out = Vec::new();
    for (idx, line) in lines.iter().take(200).enumerate() {
        let t = line.trim_start();
        if line_comment(lang)
            .map(|m| t.starts_with(m))
            .unwrap_or(false)
        {
            continue;
        }
        let words: Vec<&str> = t
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .collect();
        let mut iter = words.iter();
        let mut found: Option<(&str, &str)> = None;
        while let Some(w) = iter.next() {
            if kinds.contains(w) {
                if let Some(n) = iter.next() {
                    if n.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                        found = Some((w, n));
                    }
                }
                break;
            }
        }
        if let Some((k, n)) = found {
            out.push(SymbolInfo {
                line: (idx + 1) as u64,
                kind: k.to_string(),
                name: n.to_string(),
            });
            if out.len() >= 50 {
                break;
            }
        }
    }
    out
}

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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
pub fn install_rust_analyzer(_dest_dir: &Path) -> anyhow::Result<PathBuf> {
    anyhow::bail!("installer unavailable on wasm")
}

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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
pub fn parse_sexp(lang: LangKind, text: &str) -> Option<String> {
    let language = ts_language(lang)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(text, None).map(|t| t.root_node().to_sexp())
}

#[cfg(target_arch = "wasm32")]
pub fn parse_sexp(_lang: LangKind, _text: &str) -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
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
    out.sort_by_key(|a| (a.line, a.col));
    out
}

#[cfg(target_arch = "wasm32")]
pub fn syntax_errors(_lang: LangKind, _path: &Path, _text: &str) -> Vec<Diagnostic> {
    Vec::new()
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TsSpan {
    pub line: u64,
    pub col: u64,
    pub len: u64,
    pub kind: TokenKind,
}

fn ts_kind(name: &str) -> Option<TokenKind> {
    let base = name.split('.').next().unwrap_or(name);
    match base {
        "keyword" => Some(TokenKind::Keyword),
        "string" => Some(TokenKind::Str),
        "comment" => Some(TokenKind::Comment),
        "number" | "float" | "integer" => Some(TokenKind::Number),
        "function" | "method" | "constructor" => Some(TokenKind::Keyword),
        "type" | "class" | "interface" | "enum" => Some(TokenKind::Keyword),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn highlights_query(lang: LangKind) -> Option<&'static str> {
    match lang {
        LangKind::Rust => Some(tree_sitter_rust::HIGHLIGHTS_QUERY),
        // 0.25 renamed the const to the singular HIGHLIGHT_QUERY.
        LangKind::JavaScript => Some(tree_sitter_javascript::HIGHLIGHT_QUERY),
        LangKind::Python => Some(tree_sitter_python::HIGHLIGHTS_QUERY),
        LangKind::Go => Some(tree_sitter_go::HIGHLIGHTS_QUERY),
        // 0.23 ships one highlights.scm shared by both grammars.
        LangKind::TypeScript | LangKind::Tsx => Some(tree_sitter_typescript::HIGHLIGHTS_QUERY),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn ts_highlight(lang: LangKind, text: &str) -> Vec<TsSpan> {
    let language = match ts_language(lang) {
        Some(l) => l,
        None => return Vec::new(),
    };
    let source = match highlights_query(lang) {
        Some(q) => q,
        None => return Vec::new(),
    };
    let query = match tree_sitter::Query::new(&language, source) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(text, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut cursor = tree_sitter::QueryCursor::new();
    let names = query.capture_names();
    let mut out: Vec<TsSpan> = Vec::new();
    // tree-sitter 0.27: QueryMatches is a streaming iterator, not std Iterator.
    use tree_sitter::StreamingIterator as _;
    let mut matches = cursor.matches(&query, tree.root_node(), text.as_bytes());
    while let Some(m) = matches.next() {
        for cap in m.captures() {
            let name: &str = names[cap.index as usize];
            let Some(kind) = ts_kind(name) else {
                continue;
            };
            let node = cap.node;
            let sp = node.start_position();
            let len = node.end_byte().saturating_sub(node.start_byte()) as u64;
            if len == 0 || len > 500 {
                continue;
            }
            out.push(TsSpan {
                line: (sp.row + 1) as u64,
                col: (sp.column + 1) as u64,
                len,
                kind,
            });
            if out.len() >= 20000 {
                break;
            }
        }
        if out.len() >= 20000 {
            break;
        }
    }
    out.sort_by(|a, b| {
        (a.line, a.col, b.len)
            .cmp(&(b.line, b.col, a.len))
    });
    let mut clean: Vec<TsSpan> = Vec::new();
    let mut end_line = 0u64;
    let mut end_col = 0u64;
    for s in out {
        if s.line > end_line || (s.line == end_line && s.col >= end_col) {
            end_line = s.line;
            end_col = s.col + s.len;
            clean.push(s);
        }
    }
    clean
}

#[cfg(target_arch = "wasm32")]
pub fn ts_highlight(_lang: LangKind, _text: &str) -> Vec<TsSpan> {
    Vec::new()
}
