# Keplr — personal lightweight IDE

> Full vision doc. Frozen spec: `docs/superpowers/specs/2026-09-17-keplr-design.md`.

**What:** Rust-only, no-Electron IDE. Canvas renderer (`winit + wgpu` native, WASM + WebGPU browser) from one codebase. Zed-exact dark sleek UX. Headless `keplr serve` + desktop native with seamless sync.

**Why:** handle huge monorepos (thousands to lakhs of lines) with instant search and smart builds, for React/Vite/TSX, C++, Go, Rust, and custom LAML (`~/LAML`).

**How (3-year build):**

1. `keplr-core` — fingerprint index, watcher, fuzzy + trigram search, rope buffers.
2. `keplr-render + keplr-ui` — own canvas scene, Zed layout 1:1.
3. `keplr-sync` — Blake3 CAS shared by desktop/browser, `yrs` CRDT buffers, Git + Git LFS native (partial clone, lazy fetch).
4. `keplr-build` — `keplr.json` DAG, hash skipping, CAS output reuse, warm `vite`/`cargo`/`go` daemons.
5. `keplr-lang` — tree-sitter + LSP proxy; LAML LSP shells to `laml` binary, grammar from `vscode-extension/`.
6. `keplr-serve + keplr-cli` — self-hosted hub (Tailscale/ed25519), open from anywhere with warm cache.

**Repo:** Git LFS for `assets/`, `bench-corpus/`. See frozen spec for budgets, data flows, error handling, testing, eras.
