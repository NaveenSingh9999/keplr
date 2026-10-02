# Keplr UI Rebirth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Keplr a flat, macOS/VS Code-flavoured shell on both clients — rounded boxes, hairlines, shadows, motion everywhere, user-dropped JSON themes, a new native editor, and a real index page — with no cards, no duplicate nav rows and no dead zones.

**Architecture:** One design contract (a JSON theme file plus this spec's layout and motion rules), rendered twice: natively through rcus's view tree, in the browser through CSS custom properties generated from the same file. rcus grows a rounded-box SDF shader and a wake-on-demand frame clock first, because nothing else is drawable until it does. `keplr-anim` owns every animated value so motion is declared, not sprinkled.

**Tech Stack:** Rust 1.97, wgpu 22, winit 0.30, naga WGSL, serde/serde_json, tree-sitter (via `keplr-lang`), CodeMirror 6 (browser only), Node + Playwright (browser tests).

**Spec:** `docs/superpowers/specs/2026-10-02-ui-rebirth-design.md`

---

## Global Constraints

- No browser engine, DOM, or Node runtime in the native path. Keplr's native client depends only on rcus, wgpu, winit.
- `#rrggbb` or `#rrggbbaa` for every colour token. A malformed token falls back to the default theme's value; it never blanks the window.
- `radius 0` and `border_width 0` with no shadow must render **pixel-identical** to today's flat rectangle. This is the load-bearing invariant of Task 1.
- All bars flush to the window edges; panels separated by a 1px rule. No rounded container around a panel, no shadow on a panel, no floating pill.
- Exactly one navigation row. The activity bar selects the sidebar view; the sidebar does not repeat it.
- Motion is transform/opacity/colour only — never layout. Nothing loops except the shimmer.
- `motion.reducedMotion: true` collapses every duration to 0 while keeping identical end states.
- Every task ends green on all seven Keplr CI jobs and all six rcus CI jobs. No local builds; `cargo fmt` locally is the only exception.
- rcus is published to crates.io; a change to a published crate is versioned, tagged and published bottom-up before Keplr takes it.

## Review Focus

Each line is an input or condition the spec implies that no test would obviously cover. The test that pins each one is named in its task.

1. **A theme file that is missing, empty, malformed JSON, or a 4 MB file of nonsense.** The window must open with the default theme and name the offending file in the status bar — never blank, never panic. → Task 7
2. **A window resize arriving mid-animation, and a drag in progress when it arrives.** The live animation must retarget to the new box instead of finishing into the old one; a drag must not be interrupted by an animation. → Task 4
3. **A terminal emitting output faster than frames** (a `cargo build` scrollback burst). Redraws must coalesce to one per frame; the animation clock must not spin at 1000 Hz. → Task 4
4. **A file with no trailing newline, a line 40 000 columns wide, or a NUL byte.** The editor must not hang, must not panic on slicing, and must show something. → Task 15
5. **Reduce-motion switched on mid-session while a spring is mid-flight.** It must snap to the end state on the next frame, not finish the spring. → Task 4

---

## File Structure

### rcus (`~/rcus`)

| File | Responsibility |
|---|---|
| `crates/rcus-core/src/boxstyle.rs` | `Radius`, `Shadow` value types with serde |
| `crates/rcus-core/src/anim.rs` | `Curve`, `Ease`, `Spring`, `Animated<T>` — pure math, no I/O |
| `crates/rcus-core/src/style.rs` | + `radius`, `border_width`, `border_color`, `shadow` fields and builders |
| `crates/rcus-core/src/view.rs` | + `ViewNode::shadow_node`, + `scroll_box` on `ViewNode` |
| `crates/rcus-render/src/quad.rs` | `BoxVertex`, `push_box` (SDF box assembly, quad expanded for the shadow) |
| `crates/rcus-render/src/pipeline.rs` | `BOX_SHADER`, box vertex layout, box pass uses alpha blending |
| `crates/rcus-render/src/text.rs` | box fields copied onto glyph quads for atlas padding |
| `crates/rcus-layout/src/tree.rs` | `LayoutNode` + `radius`, `border_width`, `border_color`, `shadow` |
| `crates/rcus-layout/src/flex.rs` | copies the new style fields onto the node |
| `crates/rcus-app/src/anim.rs` | `MotionClock` — poll list, `due()`, `settled()` |
| `crates/rcus-app/src/lib.rs` | `App::keep_animating_until`, `App::animating`, `snapshot_png` unchanged |
| `crates/rcus-desktop/src/lib.rs` | wakes at display refresh while `app.animating()` is live |
| `examples/rounded-check/` | headless pixel assertions for radius, border, shadow |

### Keplr (`~/keplr`)

| File | Responsibility |
|---|---|
| `crates/keplr-theme/src/theme.rs` | `UserTheme` — the spec's JSON schema, typed |
| `crates/keplr-theme/src/discovery.rs` | find, parse and validate `<root>/.keplr/themes/*.json` |
| `crates/keplr-theme/src/css.rs` | `UserTheme` → `:root { --k-*: … }` for the browser |
| `crates/keplr-theme/src/lib.rs` | re-exports; `Theme::amoled()` kept for the TUI and wasm paths |
| `crates/keplr-anim/src/lib.rs` | the motion catalogue: durations, indicators, panels, stagger, shimmer |
| `crates/keplr-native/src/theme.rs` | `Chrome` built from `UserTheme` instead of `const`s |
| `crates/keplr-native/src/shell.rs` | title bar, activity bar, sidebar, editor area, panel, status bar, resizing |
| `crates/keplr-native/src/editor.rs` | native editor: highlighting, selection, caret, gutter, guides |
| `crates/keplr-native/src/welcome.rs` | the index page |
| `crates/keplr-native/src/view.rs` | assembles the shell; keeps the existing headless tests |
| `crates/keplr-native/src/state.rs` | theme, sidebar/panel sizes, selection, undo, find |
| `crates/keplr-serve/src/ui.html` | the browser shell: new layout, generated custom properties, motion |
| `crates/keplr-serve/tests/ui_theme.test.mjs` | the browser reads the same theme file |

---

### Task 1: Box style values

**Files:**
- Create: `crates/rcus-core/src/boxstyle.rs`
- Modify: `crates/rcus-core/src/style.rs`, `crates/rcus-core/src/lib.rs`

**Interfaces:**
- Consumes: `rcus_core::style::Color`
- Produces: `pub struct Radius { pub sm: f32, pub md: f32, pub lg: f32 }`, `pub struct Shadow { pub offset_x: f32, pub offset_y: f32, pub blur: f32, pub spread: f32, pub color: Color }`, `impl Default for Radius` (all `0.0`), both `Serialize + Deserialize` with camelCase field names, `Radius` defaulting to `{0,0,0}` on deserialize.

- [ ] **Step 1: Write the failing test** — in `crates/rcus-core/src/boxstyle.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_radius_defaults_to_nothing() { assert_eq!(Radius::default(), Radius { sm: 0.0, md: 0.0, lg: 0.0 }); }
    #[test]
    fn a_shadow_defaults_to_nothing() {
        let s = Shadow::default();
        assert_eq!((s.blur, s.spread, s.color.a), (0.0, 0.0, 0.0));
    }
    #[test]
    fn a_radius_decodes_from_camel_case() {
        let r: Radius = serde_json::from_str(r#"{"sm":2,"md":6,"lg":10}"#).expect("decodes");
        assert_eq!(r.md, 6.0);
    }
    #[test]
    fn a_shadow_decodes_from_camel_case() {
        let s: Shadow = serde_json::from_str(
            r#"{"offsetX":0,"offsetY":2,"blur":8,"spread":0,"color":{"r":0,"g":0,"b":0,"a":0.5}}"#,
        ).expect("decodes");
        assert_eq!(s.blur, 8.0);
    }
}
```

- [ ] **Step 2: Run the test to verify it fails** — Run: `cargo test -p rcus-core boxstyle` → Expected: FAIL, cannot find `Radius`.
- [ ] **Step 3: Implement** — declare `Radius`, `Shadow`, their `Default` impls, `Copy`, `Debug`, `PartialEq`, and serde derives in `boxstyle.rs`; `pub mod boxstyle; pub use boxstyle::{Radius, Shadow};` in `lib.rs`.
- [ ] **Step 4: Run the test to verify it passes** — Run: `cargo test -p rcus-core boxstyle` → Expected: PASS, 4 tests.
- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat(core): radius and shadow style values"`

### Task 2: Style carries the box

**Files:**
- Modify: `crates/rcus-core/src/style.rs`

**Interfaces:**
- Consumes: `Radius`, `Shadow` from Task 1
- Produces: `Style::radius: Radius`, `Style::border_width: f32`, `Style::border_color: Option<Color>`, `Style::shadow: Option<Shadow>`, and builders `.radius(f32)` (sets `md`, the axis-agnostic default), `.radius_full(Radius)`, `.border(f32, Color)`, `.shadow(Shadow)`; `Style::bordered_width()` returning `border_width.max(0.0)`.

- [ ] **Step 1: Write the failing test** — in `style.rs`'s test module:

```rust
#[test]
fn a_box_defaults_to_a_flat_rectangle() {
    let style = Style::default();
    assert_eq!(style.radius.md, 0.0);
    assert_eq!(style.border_width, 0.0);
    assert!(style.border_color.is_none());
    assert!(style.shadow.is_none());
}
#[test]
fn a_border_sets_its_width_and_colour_together() {
    let style = Style::default().border(1.0, Color::rgba(0.1, 0.1, 0.1, 1.0));
    assert_eq!(style.border_width, 1.0);
    assert!(style.border_color.is_some());
}
```

- [ ] **Step 2: Run it to verify it fails** — Run: `cargo test -p rcus-core style::tests::a_box_defaults` → Expected: FAIL, no field `radius`.
- [ ] **Step 3: Implement** — add the four fields with `#[serde(default)]`, the four builders, and re-export `Radius`/`Shadow` through `style`.
- [ ] **Step 4: Run it to verify it passes** — Run: `cargo test -p rcus-core` → Expected: PASS, all style tests.
- [ ] **Step 5: Commit** — `git commit -m "feat(core): style carries radius, border and shadow"`

### Task 3: The rounded-box pipeline

**Files:**
- Modify: `crates/rcus-render/src/quad.rs`, `crates/rcus-render/src/pipeline.rs`, `crates/rcus-render/src/lib.rs`
- Create: `crates/rcus-core/src/view.rs` addition `shadow_node`

**Interfaces:**
- Consumes: `Style`'s box fields
- Produces: `pub struct BoxVertex { position: [f32;2], local: [f32;2], half: [f32;2], radius: f32, border_width: f32, fill: [f32;4], border: [f32;4], shadow_offset: [f32;2], shadow_blur: f32, shadow_spread: f32, shadow_color: [f32;4] }` (Pod, Zeroable), `pub fn push_box(out: &mut Vec<BoxVertex>, viewport: Size, rect: Rect, radius: f32, border_width: f32, fill: [f32;4], border: [f32;4], shadow: Option<(f32,f32,f32,f32,[f32;4])>)`, `ViewNode::shadow_node(id, shadow, children)`, and `Frame.boxes: Vec<BoxVertex>` replacing `Frame.quads: Vec<Vertex>`.

- [ ] **Step 1: Write the failing test** — in `quad.rs`'s test module: `push_box` with `radius: 0.0, border_width: 0.0, shadow: None` emits 6 vertices; with a shadow of blur 8 and offset y 2, the emitted `position` values extend **8 px beyond `rect.bottom()`**, so `max_by = rect.bottom() + 2.0 + 8.0`.
- [ ] **Step 2: Run it to verify it fails** — Run: `cargo test -p rcus-render quad::tests` → Expected: FAIL, `push_box` not found.
- [ ] **Step 3: Implement `push_box`** — six vertices (two triangles) in device space, expanded by `blur + spread + |offset|` on each edge so the shadow has room; `local` is the pixel offset from the box centre, `half` the half-extents. Radius is clamped to `min(half.x, half.y)`. Border width clamped to `min(border_width, min(half.x, half.y))`.
- [ ] **Step 4: Implement `BOX_SHADER`** in `pipeline.rs`, replacing `SOLID_SHADER`. Its fragment shader evaluates the rounded-box signed distance `sd = length(max(abs(local) - half + r, 0.0)) - r` (with `r = 0` degenerating to a plain rect), antialiases the fill edge over one pixel, strokes the border by sampling `sd` against `-border_width`, and composites the shadow from `sd + spread` blurred by `shadow_blur`. **When `radius`, `border_width` and `shadow` are all zero the output must equal the old `input.color` exactly.**
- [ ] **Step 5: Switch the box pass to alpha blending** — in `TextPipeline`'s existing `blended_target`, reuse the same state for the solid pipeline: `SrcAlpha / OneMinusSrcAlpha`. Without it the antialiased edge is `REPLACE`d and corners look chopped. Text and box passes keep their existing order (boxes, then glyphs).
- [ ] **Step 6: Run it to verify it passes** — Run: `cargo test -p rcus-render` and `cargo clippy -p rcus-render --all-targets -- -D warnings` → Expected: PASS.
- [ ] **Step 7: Commit** — `git commit -m "feat(render): draw rounded boxes, borders and shadows"`

### Task 4: Layout carries the box

**Files:**
- Modify: `crates/rcus-layout/src/tree.rs`, `crates/rcus-layout/src/flex.rs`

**Interfaces:**
- Consumes: `Radius`, `Shadow`
- Produces: `LayoutNode::radius: Radius`, `::border_width: f32`, `::border_color: Option<Color>`, `::shadow: Option<Shadow>`; `LayoutNode::corner_radius() -> f32` returning `min(md, min(width, height) / 2.0)`.

- [ ] **Step 1: Write the failing test** — a node styled `.radius(40.0).width(20.0)` lays out with `corner_radius() == 10.0`, never half the box.
- [ ] **Step 2: Run it to verify it fails** — Run: `cargo run -p layout-check` → Expected: FAIL to compile, no `corner_radius`.
- [ ] **Step 3: Implement** — copy the four fields in `layout_node`'s Element arm next to `background`; add `corner_radius`.
- [ ] **Step 4: Verify** — `cargo run -p layout-check` → Expected: PASS (the existing stretch-child assertion still holds).
- [ ] **Step 5: Commit** — `git commit -m "feat(layout): carry radius, border and shadow onto nodes"`

### Task 5: Prove it renders, in pixels

**Files:**
- Create: `crates/rcus/examples/rounded-check/Cargo.toml`, `src/main.rs`
- Modify: `Cargo.toml` (workspace `members`)

**Interfaces:**
- Consumes: `App::snapshot_png`, `ViewNode`, `Style` box builders
- Produces: a headless example that exits non-zero on a pixel mismatch, run by the `smoke` CI job.

- [ ] **Step 1: Write the example** — render four 64×64 boxes on a 300×80 surface: a flat `radius 0` rect, a 16px-radius rect, the same rect with a 2px border, and the same rect with `Shadow { offset_y: 2, blur: 8, color: alpha 0.5 }`. Save the PNG next to the other showcase output.
- [ ] **Step 2: Add pixel assertions** — centre of the flat rect equals its fill exactly; the flat rect's corner pixel equals its fill exactly (no rounding leaked); the pixel one step inside the rounded rect's corner is the fill while the rect's own corner is the surface colour; the border pixel on the top edge equals the border colour within 2/255; a point 4px below a shadowed rect is darker than the surface and lighter than black.
- [ ] **Step 3: Run it** — Run: `cargo run -p rounded-check` → Expected: PASS, prints the sampled RGB values.
- [ ] **Step 4: Add it to the smoke job** — in `.github/workflows/ci.yml`, add `cargo run -p rounded-check` next to `cargo run -p layout-check`.
- [ ] **Step 5: Commit** — `git commit -m "feat: prove rounded boxes and shadows in pixels"`

### Task 6: A clock

**Files:**
- Create: `crates/rcus-core/src/anim.rs`, `crates/rcus-app/src/anim.rs`
- Modify: `crates/rcus-core/src/lib.rs`, `crates/rcus-app/src/lib.rs`

**Interfaces:**
- Produces: `pub enum Curve { Ease([f32;4]), Spring { stiffness: f32, damping: f32 } }`, `pub struct Animated<T> { from: T, to: T, start: Instant, duration: Duration, curve: Curve, value: T, done: bool }` with `Animated::new(from, to, start, duration, curve)`, `.value() -> T`, `.value_at(now: Instant) -> T`, `.settled(now: Instant) -> bool`, `.duration() -> Duration`; `MotionClock::push(deadline: Instant)`, `MotionClock::due(now: Instant) -> bool`, `MotionClock::settled(now: Instant) -> bool`, `MotionClock::clear()`; `App::keep_animating_until(Instant)`, `App::animating() -> Option<Instant>`.

- [ ] **Step 1: Write the failing tests** — in `anim.rs`: an `Animated` at `t=0` returns `from`; at `t=duration` returns `to`; a cubic ease with control points `[0,0,1,1]` is the identity curve; a spring settles within 2 seconds for stiffness 220 / damping 26 and `settled()` is false at `t=0`; `MotionClock::due` is false for an empty clock and true only while a pushed deadline is in the future.
- [ ] **Step 2: Run them to verify they fail** — Run: `cargo test -p rcus-core anim` → Expected: FAIL, not found.
- [ ] **Step 3: Implement `Animated<T: Copy + Lerp>`** — `Lerp` is a small trait `fn lerp(a: T, b: T, t: f32) -> T` implemented for `f32`, `u8`, and `[f32; 4]`. Ease solves the cubic by bisection (12 iterations) so no polynomial solver is needed; Spring integrates a damped harmonic at 240 Hz internally and clamps to `to` once it comes to rest.
- [ ] **Step 4: Implement `MotionClock` and the `App` methods** — the clock keeps a `Vec<Instant>` of deadlines and answers `due`/`settled` from it; it must never grow past 64 entries, dropping the earliest.
- [ ] **Step 5: Run them to verify they pass** — Run: `cargo test -p rcus-core -p rcus-app` → Expected: PASS.
- [ ] **Step 6: Commit** — `git commit -m "feat(core): animated values and a motion clock"`

### Task 7: The window wakes itself

**Files:**
- Modify: `crates/rcus-desktop/src/lib.rs`

**Interfaces:**
- Consumes: `App::animating()`
- Produces: `DesktopApp::refresh_hz() -> u32` (default 60), and the loop behaviour: while `app.animating()` is `Some(deadline)` the backend sets `ControlFlow::WaitUntil(min(deadline, now + 1/refresh))` and requests a frame; when `None` it returns to `ControlFlow::Wait`.

- [ ] **Step 1: Write the failing test** — `Redraw::take()` already has tests; add `fn an_idle_window_asks_for_nothing`: assert `MotionClock::settled(now)` is true for an empty clock, which is the predicate `about_to_wait` uses to decide not to request a frame.
- [ ] **Step 2: Run it to verify it fails** — Run: `cargo test -p rcus-desktop` → Expected: FAIL, no `refresh_hz`.
- [ ] **Step 3: Implement** — in `resumed`, store `refresh_hz`. In `about_to_wait`, if `self.app.animating()` is `Some(deadline)` set `WaitUntil(deadline.min(now + period))` and `request_frame()`; otherwise `Wait`. A `Redraw` request must still force a frame regardless of the clock, so an event storm coalesces into one frame per wake rather than one per request.
- [ ] **Step 4: Run it to verify it passes** — Run: `cargo test -p rcus-desktop && cargo clippy -p rcus-desktop --all-targets -- -D warnings` → Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(desktop): wake the loop while something is animating"`

### Task 8: Release rcus 0.2.0

**Files:**
- Modify: `Cargo.toml`, `crates/*/Cargo.toml`, `Cargo.lock`

**Interfaces:**
- Produces: rcus 0.2.0 on crates.io, all seven crates.

- [ ] **Step 1: Bump** — `workspace.package.version` to `0.2.0` and every internal `version = "0.1.5"` dependency to `"0.2.0"`. The minor bump is required: `Frame::quads` became `Frame::boxes` and `Style` gained fields, both breaking.
- [ ] **Step 2: Verify** — `cargo clippy --workspace --all-targets -- -D warnings && cargo build --workspace` → Expected: clean.
- [ ] **Step 3: Commit and tag** — `git commit -m "release: rcus 0.2.0" && git tag v0.2.0 && git push --follow-tags`.
- [ ] **Step 4: Publish** — Run: `gh workflow run publish.yml -f version=0.2.0`, wait for it to conclude success, then confirm every crate returns HTTP 200 from `https://crates.io/api/v1/crates/<name>/0.2.0` with a `User-Agent` header. Re-run on a 429; the window resets every 10 minutes.

---

### Task 9: The theme schema

**Files:**
- Create: `crates/keplr-theme/src/theme.rs`, `src/css.rs`, `src/discovery.rs`
- Modify: `crates/keplr-theme/src/lib.rs`, `Cargo.toml` (add `serde_json`)

**Interfaces:**
- Consumes: nothing
- Produces: `UserTheme { name: String, appearance: ThemeAppearance, ui: UiTokens, colors: ColorTokens, syntax: SyntaxTokens, motion: MotionTokens }`, all `#[serde(default)]` so a partial file is legal; `ThemeAppearance::{Dark, Light, HighContrast}`; `MotionTokens { fast_ms: u16, normal_ms: u16, slow_ms: u16, ease: [f32;4], spring: SpringTokens, reduced_motion: bool }`; `UiTokens { font_size: f32, line_height: f32, density: Density, border_width: f32, radius: Radius }`; `ColorTokens` with the 18 keys and `SyntaxTokens` with the 11 keys from the spec; `UserTheme::merged(over: &UserTheme) -> UserTheme` (per-field, `over` wins only where it differs from the default — so a theme file may set one colour); `UserTheme::to_css(&self) -> String` emitting `:root{--k-chrome:…;--k-syntax-keyword:…;…}` with 8-digit hex; `UserTheme::default()` = Keplr Dark per the spec.

- [ ] **Step 1: Write the failing tests** — `theme.rs`: a full JSON file from the spec decodes with every field equal to the spec's value; an empty object decodes to exactly `UserTheme::default()`; `{"colors":{"accent":"#f00"}}` merged over the default gives accent `#ff0000` and leaves `text` untouched; `{"motion":{"fast_ms":9999}}` clamps to 400; `appearance` rejects `"neon"` with a message naming the key; every colour key rejects `"chartreuse"` except `#`-prefixed hex.
- [ ] **Step 2: Run them to verify they fail** — Run: `cargo test -p keplr-theme` → Expected: FAIL, no `UserTheme`.
- [ ] **Step 3: Implement `theme.rs`** — every token type with `#[serde(default)]` and a `validate()` returning `Vec<String>` of human-readable problems; `merged` comparing against `UserTheme::default()`.
- [ ] **Step 4: Implement `css.rs`** — `to_css` emitting `--k-<group>-<key>` for all 29 tokens plus `--k-font-size`, `--k-radius-md`, `--k-dur-fast`, `--k-ease`, `--k-reduced`.
- [ ] **Step 5: Implement `discovery.rs`** — `pub fn discover(root: &Path) -> Vec<DiscoveredTheme>` where `DiscoveredTheme { name: String, path: PathBuf, source: ThemeSource }` and `ThemeSource::{File, BuiltIn}`; files sorted by name; a file over 256 KB is skipped with its path recorded; an unparseable file yields no theme but is reported by `pub fn problems(root: &Path) -> Vec<String>`.
- [ ] **Step 6: Run them to verify they pass** — Run: `cargo test -p keplr-theme` → Expected: PASS.
- [ ] **Step 7: Commit** — `git commit -m "feat(theme): the user theme schema, merge and CSS bridge"`

### Task 10: Themes on disk

**Files:**
- Create: `.keplr/themes/keplr-dark.json`
- Modify: `crates/keplr-serve/tests/ui_theme.test.mjs` (new)

**Interfaces:**
- Consumes: `discover`, `problems`
- Produces: a committed theme file the browser test asserts against.

- [ ] **Step 1: Write the file** — the spec's Keplr Dark, verbatim.
- [ ] **Step 2: Write the browser test** — `ui_theme.test.mjs` fetches `/theme.css`, parses the `:root` block, and asserts `--k-chrome` is `#000000` and `--k-syntax-keyword` is `#BF5AF2`, so the browser is provably reading the same file the native client reads.
- [ ] **Step 3: Serve it** — `keplr-serve` gains a `GET /theme.css` route returning `UserTheme::to_css()` for the resolved theme, and `GET /themes` returning the discovered list as JSON.
- [ ] **Step 4: Run** — Run: `cargo test -p keplr-serve` then `node crates/keplr-serve/tests/ui_theme.test.mjs` → Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(theme): serve the user's theme to both clients"`

### Task 11: The native client reads a theme

**Files:**
- Create: `crates/keplr-native/src/theme.rs`
- Modify: `crates/keplr-native/src/state.rs`, `src/view.rs`, `Cargo.toml`

**Interfaces:**
- Consumes: `UserTheme`, `Chrome`
- Produces: `pub struct Chrome` with one field per colour token (not a fixed `BG`/`ACCENT` list), `pub fn Chrome::new(&UserTheme) -> Chrome`, `pub fn chrome(&UserTheme, f32 /*font_size*/) -> Color` mapping any `#rrggbbaa` token to `rcus::Color`; `State::theme(&UserTheme)`, `State::chrome() -> Chrome`; `State::status` reporting a bad theme file's path.

- [ ] **Step 1: Write the failing test** — `theme.rs`: `Chrome::new(&UserTheme::default()).text == Color::rgba(0.902, 0.902, 0.918, 1.0)` within 1/255; a theme with `accent: "#FF0000"` gives `chrome.accent.r == 1.0`; an 8-digit token gives `a` of `0x80 / 255`.
- [ ] **Step 2: Run it to verify it fails** — Run: `cargo test -p keplr-native theme` → Expected: FAIL, no `Chrome::new`.
- [ ] **Step 3: Implement** — parse each token once; the existing `view.rs` `Chrome` struct is replaced by this one and every `chrome.bg` / `chrome.accent` call site keeps working because the field names match the token names.
- [ ] **Step 4: Wire discovery** — `State::new` calls `keplr_theme::discover(root)`, picks the theme named by `KEPLR_THEME` or the first, and appends any problem to `status`.
- [ ] **Step 5: Run it to verify it passes** — Run: `cargo test -p keplr-native` → Expected: PASS, existing 26 tests plus the new ones.
- [ ] **Step 6: Commit** — `git commit -m "feat(native): colour the window from the user's theme"`

---

### Task 12: The motion catalogue

**Files:**
- Create: `crates/keplr-anim/Cargo.toml`, `src/lib.rs`
- Modify: `Cargo.toml` (workspace `members`)

**Interfaces:**
- Consumes: `rcus::Animated`, `rcus::Curve`, `UserTheme`
- Produces: `pub struct Motion { pub fast: Duration, pub normal: Duration, pub slow: Duration, pub ease: Curve, pub spring: Curve, pub reduced: bool }` with `Motion::from(&MotionTokens)`; `pub struct Indicator { x: Animated<f32>, width: Animated<f32>, active: bool }` with `Indicator::move_to(&mut self, x, width, now, &Motion)`; `pub struct Panel { progress: Animated<f32> }` with `Panel::set_open(&mut self, bool, now, &Motion)` and `.value() -> f32`; `pub fn stagger_delay(index: usize, cap: usize) -> Duration` returning `min(index, cap) * 8ms`; `pub fn shimmer_phase(now: Instant, period: Duration) -> f32`.

- [ ] **Step 1: Write the failing tests** — reduced motion collapses `Motion::fast` to `Duration::ZERO`; `stagger_delay(20, 12) == stagger_delay(12, 12)` (the cap holds); `Indicator::move_to` halfway through a 180 ms normal transition puts `x` strictly between the old and new values; `shimmer_phase` is in `0.0..=1.0` for any input.
- [ ] **Step 2: Run them to verify they fail** — Run: `cargo test -p keplr-anim` → Expected: FAIL, no such crate.
- [ ] **Step 3: Implement** — thin wrappers over `Animated`, with `Motion::from` mapping `reduced_motion: true` to zero durations and a `Curve::Ease` identity for `ease`.
- [ ] **Step 4: Run them to verify they pass** — Run: `cargo test -p keplr-anim` → Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(anim): the motion catalogue"`

### Task 13: Activity bar and sidebar

**Files:**
- Create: `crates/keplr-native/src/shell.rs`
- Modify: `crates/keplr-native/src/state.rs`, `src/view.rs`

**Interfaces:**
- Consumes: `State::client` (`Pane`, `PaneKind`), `Motion`
- Produces: `pub enum SidebarView { Files, Search, Source, Outline }`, `State::sidebar_view`, `State::set_sidebar_view`, `State::sidebar_width: f32` (default 260.0, clamped 200.0–520.0), `State::title_height` 38.0, `State::tab_strip_height` 35.0, `State::status_height` 24.0, `State::activity_width` 48.0; `shell::view(state, rows, chrome, motion) -> ViewNode` producing ids `title-bar`, `activity-bar`, `sidebar`, `sidebar-resize`, `editor-area`, `panel`, `panel-resize`, `status-bar`.

- [ ] **Step 1: Write the failing tests** — a column of five ids exists; `title-bar` and `status-bar` are exactly 1280 wide at x=0; `activity-bar` is 48 wide; `sidebar` is 260 wide; `editor-area` starts at `308.0` (48 + 260); the sidebar's height equals the editor area's; there is **no** second nav row inside `sidebar` (assert the node's only child ids are the view title and its list); `set_sidebar_view` focuses the matching pane.
- [ ] **Step 2: Run them to verify they fails** — Run: `cargo test -p keplr-native shell` → Expected: FAIL, ids not found.
- [ ] **Step 3: Implement `shell::view`** — build each region with ids; the activity bar is a column of four 48×48 icon buttons with `radius(6.0)` on hover and the accent glyph for the selected view; the sidebar is a column of a 30px view title and a clipped list.
- [ ] **Step 4: Run them to verify they pass** — Run: `cargo test -p keplr-native` → Expected: PASS.
- [ ] **Step 5: Commit** — `git commit -m "feat(native): activity bar and sidebar, one nav row"`

### Task 14: The new editor

**Files:**
- Create: `crates/keplr-native/src/editor.rs`
- Modify: `crates/keplr-client/src/document.rs`, `src/selection.rs` (new), `src/undo.rs` (new)
- Modify: `crates/keplr-native/src/state.rs`, `src/view.rs`

**Interfaces:**
- Consumes: `keplr_lang::{LangKind, highlight, syntax_errors}`, `UserTheme::syntax`
- Produces: `keplr_client::Selection { anchor: usize, head: usize }` with `ordered() -> (usize, usize)`, `is_empty()`, `Document::selection() -> Selection`, `Document::set_selection(Selection)`, `Document::select_all()`, `Document::delete_selection() -> bool`; `keplr_client::UndoStack` with `push(&Document)`, `undo(&mut Document) -> bool`, `redo(&mut Document) -> bool`, `clear()`, `coalesce_window() -> Duration` (500 ms); `State::find: Option<Find>`, `State::find_next`, `State::find_prev`; `editor::view(state, path, rows, chrome, caret) -> ViewNode` with ids `editor`, `editor-gutter`, `editor-text`, `editor-selection-{i}`, `editor-caret`, `editor-guide-{i}`.

- [ ] **Step 1: Write the failing tests** — `selection.rs`: an empty selection deletes nothing; a selection from 2 to 5 deletes exactly three bytes and moves the cursor to 2; backspace with a selection deletes the selection rather than one character. `undo.rs`: after one insert, `undo` restores the text; a second insert within the coalesce window is undone in one step; `redo` reapplies; `undo` on a clean stack returns false. `editor.rs`: a fixture with `fn main() {}` produces at least one span whose token is `keyword`; the caret is a 2px-wide accent rect whose y equals its line's y; a selection rect's width equals `measure(selected_text)`.
- [ ] **Step 2: Run them to verify they fail** — Run: `cargo test -p keplr-client selection undo` and `cargo test -p keplr-native editor` → Expected: FAIL.
- [ ] **Step 3: Implement selection and undo in `keplr-client`** — `insert` and `backspace` first call `delete_selection`; `UndoStack` stores `(text, selection)` snapshots of documents up to 2 MiB total and drops the oldest beyond that.
- [ ] **Step 4: Implement `editor::view`** — per visible line: gutter number, indent guides at every 4 columns, the highlighted spans from `highlight()`, selection rectangles behind the spans, and one caret rect. Only the visible window is highlighted, so opening a 200k-line file stays instant.
- [ ] **Step 5: Run them to verify they pass** — Run: `cargo test -p keplr-client -p keplr-native` → Expected: PASS.
- [ ] **Step 6: Commit** — `git commit -m "feat(native): a real editor — selection, undo, highlighting"`

### Task 15: The browser shell

**Files:**
- Modify: `crates/keplr-serve/src/ui.html`
- Create: `crates/keplr-serve/tests/ui_shell.test.mjs`

**Interfaces:**
- Consumes: `/theme.css` from Task 10
- Produces: a DOM with `#title-bar`, `#activity-bar`, `#sidebar`, `#editor-area`, `#panel`, `#status-bar`; `window.__keplr_shell()` returning each region's `getBoundingClientRect()` for the test.

- [ ] **Step 1: Write the failing test** — `ui_shell.test.mjs` loads the page and asserts: `#activity-bar` is 48 wide; `#sidebar` is 260 wide; `#editor-area` starts at 308; `#title-bar` and `#status-bar` are the full window width at x=0; the sidebar's descendant count of `[role=tab]` is 0 (no second nav row); no element matching `.card` exists; every region's `bottom` meets the next region's `top` with no gap.
- [ ] **Step 2: Run it to verify it fails** — Run: `node crates/keplr-serve/tests/ui_shell.test.mjs` → Expected: FAIL, `#activity-bar` absent.
- [ ] **Step 3: Rewrite the layout** — CSS grid with `grid-template-columns: 48px var(--sidebar-w) 1fr` and `grid-template-rows: 38px 1fr var(--panel-h) 24px`; replace the card rules with flush regions separated by `1px solid var(--k-border)`; delete the floating welcome pill from the DOM and its keyframes.
- [ ] **Step 4: Replace the hardcoded `:root` block** — swap the generated `--k-*` block for the one served from `/theme.css`, keeping the existing drift test working by pointing it at the generated string.
- [ ] **Step 5: Restyle CodeMirror** — `--k-syntax-*` tokens through CodeMirror's `HighlightStyle`, `background-color: var(--k-current-line)` for the active line, and `--k-selection` for the selection.
- [ ] **Step 6: Run it to verify it passes** — Run: `node crates/keplr-serve/tests/ui_shell.test.mjs` → Expected: PASS.
- [ ] **Step 7: Commit** — `git commit -m "feat(web): the same shell, flush and un-carded"`

### Task 16: Panel, motion and the index page

**Files:**
- Create: `crates/keplr-native/src/welcome.rs`
- Modify: `crates/keplr-native/src/state.rs`, `src/view.rs`, `src/shell.rs`
- Modify: `crates/keplr-serve/src/ui.html`

**Interfaces:**
- Consumes: `keplr_anim::{Motion, Panel, Indicator, stagger_delay, shimmer_phase}`, `Chrome`
- Produces: `State::panel_height: f32` (240.0, clamped 120.0–70% of height), `State::panel_open: bool`, `State::welcome: bool`; `welcome::view(state, chrome) -> ViewNode` with ids `welcome`, `welcome-actions`, `welcome-action-{i}`; `shell::panel_progress(&Panel) -> f32`.

- [ ] **Step 1: Write the failing tests** — a closed panel contributes 0 to the editor area's height and an open one contributes `panel_height`; with reduced motion the panel's progress is 0 or 1 and never in between; the indicator's x equals the selected rail item's x after the transition settles; `welcome::view` has exactly four action nodes; `stagger_delay(12, 12)` is used for row 12 and beyond.
- [ ] **Step 2: Run them to verify they fail** — Run: `cargo test -p keplr-native panel welcome` → Expected: FAIL.
- [ ] **Step 3: Implement the panel and the indicator** — the panel's height is `panel_height * progress`; the activity-bar indicator is a 2px accent rule whose x and width are `Indicator`'s animated values; both call `state.keep_animating_until(deadline)` so the window keeps waking.
- [ ] **Step 4: Implement `welcome::view`** — project name and root, a recent-files list with stagger, and four actions: open file, open folder, start terminal, change theme. Rendered in the editor area when no file is open.
- [ ] **Step 5: Apply the same in the browser** — the panel and the sidebar animate with CSS transitions on the same durations and cubic-bezier from `/theme.css`; the welcome page becomes the `?` route's default body instead of a pill.
- [ ] **Step 6: Run it to verify it passes** — Run: `cargo test -p keplr-native` and the browser tests → Expected: PASS.
- [ ] **Step 7: Commit** — `git commit -m "feat: panel, motion and the index page"`

### Task 17: Everything visible, in CI

**Files:**
- Modify: `crates/keplr-native/src/snapshot.rs`, `.github/workflows/showcase.yml`

**Interfaces:**
- Produces: showcase PNGs for `welcome`, `files`, `editor`, `terminal`, `panel`, and both themes, plus `layout.txt`.

- [ ] **Step 1: Add the panes** — `--pane` gains `welcome`; the default set becomes `files, editor, terminal, panel, welcome`.
- [ ] **Step 2: Add a theme flag** — `--theme <name>` picks from `discover`, defaulting to the first; the showcase runs it twice, once per committed theme.
- [ ] **Step 3: Update the workflow** — run `--snapshot` for both themes and upload `showcase-output/**` plus each `layout.txt`.
- [ ] **Step 4: Verify** — Run: `gh workflow run showcase.yml`, then download the artifact and read every PNG. Expected: each region flush to its neighbour, one nav row, no floating pill, no dead zone, and text legible at 1×.
- [ ] **Step 5: Commit** — `git commit -m "feat: showcase every pane in both themes"`

### Task 18: Nothing lost

**Files:**
- Modify: `crates/keplr-native/src/view.rs` (test module)

**Interfaces:**
- Consumes: the whole shell
- Produces: a regression test naming the five rules the screenshots broke.

- [ ] **Step 1: Write the tests** — one nav row; bars flush to both edges; no region has a height of 0 when open; the editor area's bottom equals the panel's top; the status bar's text is inside its own bounds; no node id contains `card` or `pill`.
- [ ] **Step 2: Run them** — Expected: PASS. If any fails, the layout regressed and this is the gate.
- [ ] **Step 3: Delete what the redesign replaced** — remove the old card CSS and the floating pill from `ui.html`, and the superseded view functions from `view.rs`, so the old look cannot come back.
- [ ] **Step 4: Full verification** — Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets`, the browser tests, and all seven CI jobs → Expected: green.
- [ ] **Step 5: Commit** — `git commit -m "refactor: remove the card layout and the floating pill"`

---

## Self-Review

**Spec coverage.** §3.1 box model → Tasks 1–5. §3.2 clock → Tasks 6–7. §4 theme schema → Tasks 9–10. §5 shell → Task 13, browser Task 15. §6 editor → Task 14. §7 motion → Task 12, applied Task 16. §8 index page → Task 16. §9 one system two renderings → Tasks 10, 11, 15. §11 verification → Task 5, 18, every task's test steps, and Task 17 for the pictures. §12 risks are stated, not worked around. No gaps.

**Task coupling.** Tasks 1→2→3→4→5 form one chain: 5 cannot be written before 4, and 4 before 3. Tasks 6→7 are independent of 1–5 and could run in parallel. Tasks 9→10→11 and 12→16 are independent of the rcus chain. Task 14 needs 11 (theme) for its colours. Task 17 needs everything.

**Type consistency.** `Radius`, `Shadow`, `Chrome`, `Motion`, `Panel`, `Indicator`, `Selection`, `UndoStack`, `UserTheme`, `find_theme`, `SidebarView` are each defined once, in the task named in its Interfaces block. `Chrome`'s field names equal the theme's token names in snake_case, so `chrome.text` and `colors.text` cannot drift.

**Proportion.** ~18 tasks, five steps each, no code bodies except the four tests that pin exact values and the shader, which is the one algorithm a signature cannot determine.