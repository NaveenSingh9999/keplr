# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

## Users

Developers and embedded/ systems engineers who need a fast personal IDE for large repositories, local machines, remote hosts, browsers, and touch devices.

## Product Purpose

Keplr provides one lightweight Rust-native workbench for editing files, searching repositories, running real shells, working with source control, and building projects without an Electron runtime.

## Positioning

A single-binary, local-first IDE that keeps browser, native, WASM, and terminal surfaces coherent while staying fast on servers, Chromebooks, tablets, and embedded benches.

## Operating Context

Users open a workspace, navigate dense file trees, edit and preview files, run independent PTY sessions, inspect diagnostics and tasks, use source control, and rearrange work surfaces for the task at hand.

## Capabilities and Constraints

- Existing default workbench layout and saved layout state must remain compatible.
- Browser, native, WASM, and TUI surfaces share commands and layout data where platform capabilities allow.
- Terminal sessions are host-local; terminal profiles and layouts may be persisted, while live PTYs are not synced as portable state.
- CodeMirror and existing server routes remain supported during migration.
- No Electron, Node runtime, or cloud dependency is required for the core product.

## Brand Commitments

- Keplr is a serious, compact developer tool, not a dashboard or marketing surface.
- Preserve familiar editor affordances while making layout and interaction more fluid.
- The user explicitly prefers an AMOLED-based, subtly grained, translucent liquid-glass treatment over fully transparent or decorative glass.
- The existing header remains the global chrome; no menubar is added.
- The current layout is the default, and every view can become a movable card.
- Interface copy and controls should be compact, functional, and free of generic AI-product language.

## Evidence on Hand

- Product capabilities and architecture are documented in `README.md`, `docs/KEPLR.md`, and the existing Rust crates.
- Existing visual tokens and browser UI are in `crates/keplr-theme/src/lib.rs` and `crates/keplr-serve/src/ui.html`.
- Existing terminal implementation uses `portable-pty` and `alacritty_terminal` in `crates/keplr-serve/src/lib.rs`.

## Product Principles

- Preserve momentum: the workbench opens into the current usable layout.
- Make complexity spatial: users compose the workspace instead of navigating a rigid dashboard.
- Make motion meaningful: movement explains focus, hierarchy, continuity, and feedback.
- Keep dense work legible: compact controls and virtualized lists serve large repositories.
- Degrade honestly: unsupported platform capabilities have clear, recoverable states.

## Accessibility & Inclusion

Keyboard navigation, visible focus, reduced-motion behavior, reduced-transparency fallback, screen-reader labels for controls, and touch-sized hit targets are required for the workbench surfaces.
