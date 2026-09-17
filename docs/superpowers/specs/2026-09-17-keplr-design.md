# Keplr — Design Spec (2026-09-17)

Status: approved sections 1-6, awaiting spec review before implementation plan.
Scope: 3-year solo build. No YAGNI cuts. Full vision documented; implementation phases are eras, not slop versions.
Decisions locked: Rust everywhere, canvas renderer (C approach) with single-codebase mentality (A), Zed-exact UX, Git LFS native, seamless desktop+browser sync via shared cache.

## 1. Vision

Keplr is a super-lightweight personal IDE for huge monorepos (thousands to lakhs of lines):

- Headless browser mode (`keplr serve` -> open from anywhere) + desktop native, same exact UI/UX, same Rust code.
- No Electron, no Chromium shell, no Codeium dependency. Pure own stack.
- Instant searching, fast open, smart task running (not custom compilers).
- First-class languages: React / Vite / TSX, C++, Go, Rust, custom LAML (`~/LAML`, C++20 `ng/src`, `vscode-extension/` grammar reused).
- Minimal modern sleek dark UI, Zed-exact layout.
- Git LFS for repo management, local-first sync with cache sharing between desktop and browser.

Non-goals: beating `rustc`/`vite` at codegen, public cloud SaaS, plugin marketplace in early eras, AI autocomplete core dependency.

## 2. Architecture (C engine, A soul)

No DOM. No WebView. Own `keplr-render` (scene graph + `cosmic-text` shaping + `wgpu`).

- Native: `winit + wgpu` binary.
- Browser: same Rust UI compiled to WASM + WebGPU canvas.
- Headless: same core as Axum daemon streaming scene diffs + `yrs` buffer updates to WASM client.

```
┌──────────── keplr-ui (single Rust UI: tree/tabs/editor/terminal/palette) ────────────┐
│ keplr-render (layout + paint + theme tokens + input)  │  keplr-lang / keplr-build     │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ keplr-core (workspace, watcher, index, buffers) │ keplr-sync (CAS + yrs + git-lfs)    │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ keplr-serve (Axum daemon + WASM host + native shell)  │  keplr-cli                    │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

Workspace layout (`~/keplr/`):

```
keplr/
  Cargo.toml (workspace)
  keplr.json (own dogfood tasks)
  crates/
    keplr-core/    # workspace model, notify watcher, fingerprint db, search pools
    keplr-render/  # canvas scene, text layout, dark tokens, GPU abstraction + software fallback
    keplr-ui/      # Zed-exact components
    keplr-lang/    # tree-sitter + LSP proxy + laml-lsp
    keplr-build/   # DAG tasks + hash cache
    keplr-sync/    # CAS + snapshots + transport + git-lfs driver
    keplr-serve/   # headless daemon, auth, WASM bundle serve
    keplr-cli/     # open/serve/search/run
  docs/
    KEPLR.md (this vision, human entry)
    superpowers/specs/2026-09-17-keplr-design.md (this file, frozen spec)
  bench-corpus/ (LFS: synth 100k-file monorepo + LAML ng/examples mirror)
  assets/ (LFS: fonts, snapshots)
```

## 3. Monorepo index + instant search

Budget: 100k files / 5M LOC open without UI jank; 500MB soft RAM cap; results stream in <16ms chunks.

- Walk: parallel `ignore`-crate walk, gitignore-aware. Fingerprint DB `redb`: path, mtime, size, blake3.
- Watch: `notify` incremental re-hash only changed files. Corrupt index rebuilds in background; UI serves stale + badge.
- File finder (`cmd+P`): async fuzzy over paths, scored, cancel-safe.
- Content search: trigram index for hot files + streaming grep for cold with early-cancel. Dedicated search pool; UI reads via lock-free channel, never blocks render thread.
- Huge file: rope + line-index, only ~200 visible lines shaped. LRU evict to CAS/disk.

Data flow: watcher -> fingerprint -> index -> query bus -> canvas list.

## 4. Canvas UI — Zed-exact

Pixel-identical from one codebase:

- Top titlebar with centered file finder, left dock (project panel, outline, search), center tabs + breadcrumbs + editor, right dock (symbols), bottom dock (terminal + diagnostics), `cmd+shift+P` palette, status bar (branch, LSP, errors).
- Editor: rope, multi-cursor, soft-wrap, inlay hints, diagnostics squiggles, minimap off by default (Zed parity), vim-mode flag reserved.
- Input: own Zed-default keymap, `winit` IME, touch fallback in mobile browser.
- Theming: dark sleek token set only; JetBrains-Mono fallback stack.
- Resilience: GPU failure falls back to software rasterizer, never white-screen. 60fps minimum, 120fps native target.

## 5. Sync + cache sharing + Git LFS

Local-first. Git remains source of truth. Hub is your own `keplr serve` (home machine / VPS over Tailscale/WireGuard or ed25519 token).

- CAS: Blake3-chunked store `~/.keplr/cas` for file blocks, LFS objects, build outputs. Same IDs everywhere; desktop fetch warms browser.
- Buffers: open edits as `yrs` CRDT docs over WebSocket. Offline merge, snapshots flushed to disk + Git on save.
- Roaming: desktop + browser exchange only missing chunks. Open from anywhere warms in seconds.
- Git LFS native: partial-clone + lazy fetch. Large files never block open; LFS objects deduped in CAS.
- Conflicts: CRDT auto-merges buffers; external `git pull` race keeps both revisions with Zed-style banner + diff choice.

## 6. Smart task runner

Keplr orchestrates, does not recompile languages.

- `keplr.json` root + per-package: `cmd`, `watch`, `outputs`, `fingerprint`.
- `keplr-build`: DAG scheduler, parallel, content-hash skipping, CAS output reuse across machines.
- Warm daemons: `vite dev`, `cargo check -w`, `gopls` held alive over pipe, no cold start.
- UI: bottom-dock task list with live logs; failure jumps to file:line; stderr cached by hash for instant replay. Failed node cancels dependents.

## 7. Languages — LAML first-class

- TSX/React/Vite, C++, Go, Rust: tree-sitter highlight + managed LSP proxy (`typescript-language-server`, `clangd`, `gopls`, `rust-analyzer`, auto-download).
- LAML (`.lm`): grammar converted from `~/LAML/vscode-extension` + reference `~/LAML/ng/src` lexer/parser; `laml-lsp` in Rust shells to C++20 `laml` binary for `check/run`; diagnostics mapped to squiggles; completions for `serve/on/send/broadcast/joinRoom/members`, timers/JSON (`jsonParse/jsonStringify/setTimeout/async/waitFor`), `closc`, stdlib (`sort/pop/join/upper/lower/keys/has/assert`), `~` comments; run lens + REPL panel. Missing binary degrades to highlight + install hint, never hard error.

## 8. Data flow (end to end)

keystroke -> rope edit -> yrs doc -> incremental tree-sitter parse -> LSP/diagnostics -> scene diff -> wgpu paint; save -> CAS put -> fingerprint update -> git write -> sync broadcast -> build fingerprint check -> task trigger.

## 9. Error handling

Index corrupt: background rebuild. GPU lost: software fallback. LSP crash: supervisor restart + badge. Sync split-brain: CRDT merge + banner. LFS fetch fail: placeholder node + retry. Build fail: dependent cancel + cached log.

## 10. Testing + benches

`cargo test` per crate; `bench-corpus` synth + `~/LAML/ng/examples` mirror (LFS); tracked metrics: cold open 100k files, `cmd+P` p95, content-search p95, scroll fps, CAS hit rate, build skip rate. Render snapshots via headless wgpu.

## 11. Three-year eras

- Era 1 — Core canvas + open/search/edit + Zed shell + local Git LFS.
- Era 2 — Sync roam + CAS share + headless serve + WASM parity.
- Era 3 — Build DAG + CAS reuse + LAML LSP depth + perf hardening to lakhs LOC.

Each era shippable daily as personal IDE; no throwaway prototypes.

## 12. Self-review

Placeholder scan: none. Consistency: single UI codebase satisfies same-UX constraint; CAS shared by sync/LFS/build as specified; LAML reuses existing grammar/binary, no evaluator fork. Ambiguity resolved: fastest compilation means orchestration + cache, not compiler fork; sync hub is self-hosted, not SaaS.
