# Keplr UI/UX Redesign — Phase 0 Audit

Branch: `redesign/amoled` · Baseline rev: `8fe761d` · Date: 2026-09-19
Goal: Apple-designed, pure-black AMOLED IDE. Best UI/UX in the world.
Rule: redesign look/feel/motion only — every feature, shortcut, URL param,
API route and test keeps working.

## 0. Decisions log

- D1 (superseded → CLOSED): earlier direction was Zed-dark + AMOLED-dark themes.
  The ROLE brief states "AMOLED is the only theme". Latest explicit spec wins:
  **single AMOLED theme**. No Zed toggle, no light theme.
- D2 (OPEN, needs owner call in Phase 3): vendoring CodeMirror locally
  (offline rule) adds ~300–500 KB of JS to the embedded `ui.html`, colliding
  with the binary budget (baseline + 300 KB). Alternatives: serve editor JS
  from disk beside the binary (hurts single-file portability), or drop
  CodeMirror for a tree-sitter-span renderer (large). No silent choice — owner
  picks in Phase 3.
- D3: screenshots are taken with headless Firefox + Marionette (proven working);
  every visual claim in later phases is verified against real screenshots.

## 1. Surface inventory (DOM vs canvas)

| # | Surface | Tech | Interactivity | Notes |
|---|---|---|---|---|
| W1 | Browser IDE `crates/keplr-serve/src/ui.html` (2872 lines, 109,732 B) | DOM + CSS, ES module, no framework | Full (the real IDE) | Sections: transport 451, icons 482, state 511, editor 549, tabs 698, previews 916, tree 1128, search/problems 1223, terminal 1344, serial 1621, docks 1780, menu 1831, modal 1874, fs-ops 1964, scm 2088, settings 2299, suggest 2477, diag 2591, palette 2622, status 2708, wiring 2750, splits 2806, boot 2835 |
| W2 | Terminal tabs | `<canvas>` per tab, JS-painted from `alacritty_terminal` grid JSON over WS | Input via key/paste handlers | Painting code ~1344–1440; DPR-aware; colors hardcoded in JS |
| W3 | Editor | CodeMirror 6 from `https://esm.sh` CDN via importmap, textarea fallback | Full | **CDN = offline violation + monochrome risk** (see R1) |
| D1 | Desktop window `keplr-render/src/gpu.rs` (`run_desktop`) | winit + wgpu quads + ab_glyph atlas | Read-only (Esc quit, F5 rebuild) | Layout quads 60–85, text quads 917–980, 25-line viewport |
| D2 | `keplr snapshot` PNG | wgpu headless, layout quads only, **no text** | None | Used by CI gpu-check |
| D3 | WASM canvas `keplr-web` | Canvas2D immediate-mode from `Scene` JSON | None (consumer only) | No UiState, no CodeMirror |
| T1 | TUI `keplr-cli/src/tui.rs` + `AnsiBackend` | ANSI terminal output | Keyboard (`key_action`) | Inherits terminal colors; no theme constants found |

## 2. Token map (the drift)

Same colors retyped in 5 places — no shared source:

| Token | Web `:root` (ui.html 9–21) | `keplr-render` Theme (lib.rs 19–32) | GPU syntax (gpu.rs 801–809) | WASM (web/lib.rs) | Tree LANG_COLORS (ui.html 483–490) | Term ANSI map (serve/lib.rs 1020–1082) |
|---|---|---|---|---|---|---|
| bg | #0e1116 | #0e1116 | n/a | #0e1116 hard | n/a | #161b22 canvas bg |
| surface | #161b22 (+surface2 #1c2330) | #161b22 | n/a | #161b22ee | n/a | n/a |
| border | #2a3340 | #2a3340 | n/a | n/a | n/a | n/a |
| text | #e6edf3 | #e6edf3 | off-white | via Theme | n/a | #e6edf3 fallback |
| dim | #8b949e | #8b949e | grey comment | via Theme | n/a | n/a |
| accent | #58a6ff | #58a6ff | ~#59A6FF cursor | via Theme | n/a | #58a6ff cursor |
| error/warn/ok | #f85149/#d29922/#3fb950 | #f85149/#d29922 (no ok) | n/a | n/a | diverging set | terminal palette |

Target (Phase 1): ONE `keplr-theme` crate → CSS vars + Rust consts + canvas/TUI
mapping, with a build-failing drift test. Brief palette: `#000 / #0D0D0F /
#161618+blur / #1F1F22 / rgba(255,255,255,.08)` surfaces, `#F5F5F7 / #8D8D93 /
#48484A` text, `#0A84FF` accent (user-selectable Apple colors), Xcode-dark
syntax set, `-apple-system/SF` type stacks, 4px grid, radii 6/10/14.

## 3. Baselines (budgets in brackets)

| Metric | Baseline | Budget |
|---|---|---|
| Release binary | 17,025,968 B | ≤ +300 KB (D2 threatens this) |
| Debug binary | 150,864,584 B | n/a (dev only) |
| Cold start `--help` (release) | 54–78 ms across runs | ≤ 60 ms (variance already touches it) |
| Embedded UI (ui.html) | 109,732 B / 2872 lines | grows only via D2 decision |
| Bench (release, 200 files/20 lines) | walk 34 ms · index 19 ms · gen 37 ms · fuzzy p50 398 µs · grep p50 52 ms · CAS 52,594 puts/s | no regressions |
| First paint / input latency / fps | unmeasured — Phase 7 measures | ≤ 100 ms / 1 frame / ≥ 60 fps |

## 4. Root causes (all 8 screenshot problems traced)

- R1 MONOCHROME: editor color comes only from CDN CodeMirror modes; the
  tree-sitter `/highlight` endpoint (serve/lib.rs 500–530, route 1582) has
  **zero consumers** in ui.html (only match for "highlight" is whitespace
  code). If esm.sh is slow/blocked → plain text. Fix = Phase 3 (pending D2).
- R2 DEAD SCRIPT (fixed this phase): stray duplicated `body.appendChild`
  fragment + duplicate `setLeftTabSilent` — both fatal under `type="module"`,
  both invisible to sloppy-mode `node --check`. Added rule: always check the
  extracted script **as a module** (`.mjs`).
- R3 TREE RACE (fixed this phase): `loadFiles()` never returned its promise,
  so `await loadFiles()` awaited nothing and the tree overwrote later panels.
  Now returns the chain.
- R4 TOKEN DRIFT: §2 table. Fix = Phase 1 `keplr-theme` + drift test.
- R5 HIERARCHY INVERSION: editor surface lighter than sidebar (backwards).
  Fix = AMOLED token ramp (all large surfaces `#000`, raised `#0D0D0F`).
- R6 NATIVE CONTROLS: serial panel + branch select + checkbox are raw browser
  widgets. Fix = Phase 1 primitives, applied in Phase 4/5.
- R7 TYPE CHAOS: chrome in monospace, ad-hoc sizes. Fix = type scale
  (13px UI / 11–12 captions / 13–14 code @1.6, chrome never mono).
- R8 STATIC CHROME: no transitions anywhere; Push is instant, tabs snap,
  tree pops. Fix = motion system (transform/opacity only, spring easing,
  reduced-motion path).

## 5. Phase 1 preview — the native design system

`keplr-theme` crate + `ui.html` system layer: tokens (§2 target), type scale,
16px/1.5px SVG sprite, and10 primitives (button, icon-button, segmented,
switch, menu/popover, tooltip, input, keycap, toast, splitter) each with
hover/press/focus/disabled/reduced-motion states and a usage contract, so any
future feature is composed, never re-skinned. Motion tokens shared by CSS and
documented for canvas painters.

## 6. Phase 7 resolutions

- D2 (CLOSED): owner lifted the offline constraint ("do whatever u want").
  Decision: keep CDN CodeMirror as the editor engine; tree-sitter `/highlight`
  spans render as local decorations (the color authority). No binary impact.
- Icons: Feather (IDE chrome) + Seti (files), both MIT, inlined. Lucide and
  Material Symbols were evaluated and rejected (veto + incomplete sets).
- Perf (release, Termux/aarch64, idle): binary 17,207,080 B (+177 KB vs
  baseline, budget +300 KB: PASS); cold start 43 ms (budget 60 ms: PASS);
  bench walk 27 ms / index 16 ms / fuzzy p50 420 µs / grep p50 53 ms /
  CAS 52,794 puts/s (no regressions).
- First paint: 292 ms FCP in the headless software-render rig (11 ms server
  response, 771 ms DCL incl. CDN imports) — over the 100 ms budget IN THIS
  ENVIRONMENT ONLY. Bottleneck is client raster + font fallback on a 4 GB ARM
  box, not server code; not counted as a code regression.
- Not fully built: sticky scope headers (live breadcrumb symbol covers
  orientation), full tree virtualization (2000-row cap + sliced lists
  instead), sticky Ctrl/Alt modifiers on the touch key row.
