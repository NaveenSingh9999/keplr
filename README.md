# Keplr

Personal lightweight Rust IDE — canvas foundation, Zed-exact dark UX,
headless `serve` + desktop native from one codebase, Git LFS native,
LAML first-class.

Spec: `docs/superpowers/specs/2026-09-17-keplr-design.md`
Plan foundation: `docs/superpowers/plans/2026-09-18-keplr-production-foundation.md`
Plan B canvas: `docs/superpowers/plans/2026-09-18-keplr-plan-b-canvas-foundation.md`
Plan C builds: `docs/superpowers/plans/2026-09-18-keplr-build-dag.md`
Plan D index/lang/sync: `docs/superpowers/plans/2026-09-18-keplr-plan-d-index-lang-sync.md`
Plan E Zed depth: `docs/superpowers/plans/2026-09-18-keplr-plan-e-zed-depth.md`
Plan F save/bench/auth/tui/gpu: `docs/superpowers/plans/2026-09-18-keplr-plan-f-save-bench-auth-tui-gpu.md`
Plan G all-in: `docs/superpowers/plans/2026-09-18-keplr-plan-g-all-in.md`
Plan H roaming/wasm/gpu-text: `docs/superpowers/plans/2026-09-18-keplr-plan-h-roaming-wasm-gputext.md`
Plan I all remaining: `docs/superpowers/plans/2026-09-18-keplr-plan-i-all-remaining.md`
Plan J leftovers: `docs/superpowers/plans/2026-09-19-keplr-plan-j-leftovers.md`
Remote ops: `docs/REMOTE.md`

Fonts: SF Mono when macOS/Xcode provides it (Apple license, never vendored), else vendored JetBrains Mono OFL (`assets/fonts/`), else system monos. Override with `KEPLR_FONT`. `keplr fonts` shows the resolved stack.
Icon: orbit mark in Zed-dark tokens (`assets/icon/keplr.svg`, PNGs 16–512).

## Use (foundation CLI, production)

```bash
cargo run -p keplr-cli -- --root ~/LAML files "serve" --limit 20
cargo run -p keplr-cli -- --root ~/LAML search "broadcast" --limit 20
cargo run -p keplr-cli -- --root . run check
cargo run -p keplr-cli -- --root . serve --port 7137
curl '127.0.0.1:7137/search?needle=hello&limit=5'
```

## Use (Plan B canvas foundation, production)

```bash
cargo run -p keplr-cli -- --root . ui --open Cargo.toml --width 100
cargo run -p keplr-cli -- --root . scene --open Cargo.toml | head -n 40
cargo run -p keplr-cli -- --root . ui --palette "main" --width 100
cargo run -p keplr-cli -- --root . serve --port 7137 &
curl '127.0.0.1:7137/scene?open=Cargo.toml&width=100'
curl '127.0.0.1:7137/files?query=keplr&limit=5'
curl '127.0.0.1:7137/tasks'
```

## Use (Plan C smart builds, production)

```bash
cargo run -p keplr-cli -- --root . run lint
cargo run -p keplr-cli -- --root . run --all --jobs 4
cargo run -p keplr-cli -- --root . run lint --force
cargo run -p keplr-cli -- --root . run --all --watch
curl '127.0.0.1:7137/tasks/graph'
curl -X POST 127.0.0.1:7137/tasks/run -H 'Content-Type: application/json' -d '{"all":true,"jobs":4}'
```

## Use (Plan D index + watcher, production)

```bash
cargo run -p keplr-cli -- --root . index
cargo run -p keplr-cli -- --root . index --refresh
cargo run -p keplr-cli -- --root . search --via trigram "broadcast" --limit 20
cargo run -p keplr-cli -- --root . search --via index "broadcast" --limit 20
cargo run -p keplr-cli -- --root . watch --debounce-ms 500
curl '127.0.0.1:7137/index/status'
```

## Use (Plan D lang, production)

```bash
cargo run -p keplr-cli -- --root ~/LAML diagnostics ng/src/main.lm
curl '127.0.0.1:7137/diagnostics?path=Cargo.toml'
curl '127.0.0.1:7137/highlight?path=Cargo.toml&line=1'
```

## Use (Plan D sync, production)

```bash
curl '127.0.0.1:7137/lfs/pointer?path=assets/font.woff2'
curl -X POST 127.0.0.1:7137/sync/merge -H 'Content-Type: application/json' -d '{"name":"notes","seed":"hello","updates":[]}'
curl -X POST 127.0.0.1:7137/sync/snapshot -H 'Content-Type: application/json' -d '{"name":"notes","update":[1,2,3]}'
curl '127.0.0.1:7137/sync/snapshot?name=notes'
```

## Use (Plan E Zed depth, production)

```bash
cargo run -p keplr-cli -- --root . ui --open Cargo.toml --left-tab search --search "clap" --width 100
cargo run -p keplr-cli -- --root . ui --palette "run" --palette-mode commands --width 100
cargo run -p keplr-cli -- --root . scene --open crates/keplr-cli/src/main.rs --bottom-tab tasks | head -n 60
curl '127.0.0.1:7137/scene?open=Cargo.toml&left=search&search=clap&width=100'
curl '127.0.0.1:7137/scene?palette_mode=commands&palette=run'
```

## Use (Plan F save/bench/auth/tui/gpu, production)

```bash
echo 'hello keplr' | cargo run -p keplr-cli -- --root . save notes.txt --stdin
cargo run -p keplr-cli -- --root . save notes.txt --content "hello" --task lint
curl -X POST 127.0.0.1:7137/save -H 'Content-Type: application/json' -d '{"path":"notes.txt","content":"hi"}'
cargo run -p keplr-cli -- --root /tmp/bench bench --files 2000 --lines 40
cargo run -p keplr-cli -- --root . token --save
cargo run -p keplr-cli -- --root . serve --port 7137  # KEPLR_TOKEN=... or --token ...
cargo run -p keplr-cli -- --root . edit Cargo.toml
cargo run -p keplr-cli --features desktop -- --root . desktop --open Cargo.toml  # native window; falls back to ANSI without GPU
```

WASM browser parity is deferred: `notify`/`ignore` are not wasm-safe, so the workspace needs dep surgery first. The browser contract is already live — `Scene` serde JSON over `keplr serve` (see `/scene`). `keplr-lang` alone is wasm-verified in CI (`wasm-check`).

## Use (Plan G all-in, production)

```bash
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

## Use (browser UI, production)

`keplr serve` also hosts a built-in browser page (no build step, vanilla JS over the same JSON API):

```bash
cargo run -p keplr-cli -- --root ~/LAML serve --port 7137
```

Open `http://127.0.0.1:7137/` — file tree, highlighted editor, content search, task list with run buttons, git-backed files, and a real terminal tab (PTY shell parsed by `alacritty_terminal`, painted on canvas in the Zed theme — no xterm.js). Right-click anywhere in the tree, tabs, or empty space for file/folder actions (new, rename, delete, duplicate, copy path, reveal); the source tab is a VSCode-style git panel (stage/unstage/discard/commit/branches/stash). Markdown files preview with full formatting, PDFs and images open in viewers. The gear opens settings (font, wrap, suggestions, auto-save, animations) applied live. If a token is configured, paste it into the token box (or open `/?token=...`); the page itself is public, the API stays gated.

In a Codespace: run the same command in a terminal, then open the **Ports** panel, forward port `7137`, and **Open in Browser`.

## Use (Plan H roaming/wasm/gpu-text, production)

```bash
cargo run -p keplr-cli -- --root . serve --port 7137 &
cargo run -p keplr-cli -- --root /tmp/roam sync --url ws://127.0.0.1:7137/sync/channel --name notes --file notes.txt --once
cargo run -p keplr-cli --features desktop -- --root . desktop --open Cargo.toml  # editor text now GPU-painted
```

## Use (Plan I all remaining, production)

```bash
cargo run -p keplr-cli -- --root . parse crates/keplr-cli/src/main.rs | head -c 400
cargo run -p keplr-cli -- --root . diagnostics src/app.rs
cargo run -p keplr-cli -- --root . edit src/main.rs --vim
cargo run -p keplr-cli -- --root . sync --url ws://127.0.0.1:7137/sync/channel --name notes --file notes.txt
cargo run -p keplr-cli --features desktop -- --root . snapshot --open Cargo.toml --out /tmp/shot.png
```

TUI vim (`--vim`) covers hjkl/wbe/0/$/^/gg/G, i/a/I/A/o/O, x/D/dd/yy/p/P, r, d/y + w/$, `:w :q :wq :x :q!`, multi-cursor with ctrl+d in insert mode. No undo stack — use `git checkout` to revert (stated in-editor on `u`).

## Use (Plan J leftovers, production)

```bash
cargo run -p keplr-cli -- --root . lfs ls-files
cargo run -p keplr-cli -- --root . sync --url ws://127.0.0.1:7137/sync/channel --name notes  # auto-reconnects
curl '127.0.0.1:7137/highlight?path=src/main.rs&full=1' | head -c 300
curl -X POST 127.0.0.1:7137/index/rebuild
```

CI runs `cargo test --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build --workspace` on every push.
