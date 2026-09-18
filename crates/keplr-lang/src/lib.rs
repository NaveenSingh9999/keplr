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

pub fn symbols_for(lang: LangKind, lines: &[String]) -> Vec<String> {
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
    let mut out = Vec::new();
    for line in lines.iter().take(200) {
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
            out.push(format!("{k} {n}"));
            if out.len() >= 50 {
                break;
            }
        }
    }
    out
}
