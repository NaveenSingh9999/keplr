# Keplr UI rebirth — design spec

Date: 2026-10-02
Status: for review
Scope: the native (rcus) client and the browser client, one design system, two renderings.

---

## 1. What exists today, measured

Everything below was read out of the code or out of a screenshot, not assumed.

**rcus can draw** flat axis-aligned rectangles with alpha, and text glyphs from a
bitmap atlas, with per-node scissor clipping. That is the whole primitive set.
It cannot draw rounded corners, borders, shadows, gradients, or any shape that is
not a rectangle, and it has no clock: nothing animates because no frame is ever
requested without an input event.

**The native client** is `keplr-native`: a `State`, a pure `view(state) -> ViewNode`,
and a winit/wgpu window. 26 headless tests. It has no syntax highlighting, no
selection, no undo, no find, and its colours are `const` in `view.rs`.

**The browser client** is `crates/keplr-serve/src/ui.html`: 6445 hand-written lines,
1070 of CSS with `:root` custom properties, `@keyframes`, and
`prefers-reduced-motion` already honoured. Its editor is **CodeMirror 6**. It has
the cards, the duplicate nav rows, and the floating welcome pill visible in
`docs/showcase/shots/shot-editor.png`.

**Shared already**: `keplr-theme` (AMOLED tokens, with a drift test that fails the
build if `ui.html` disagrees), `keplr-lang` (tree-sitter for Rust, JavaScript,
Python, Go, TypeScript; `highlight()`, `syntax_errors()`, snippets, symbols,
completions, LSP servers), `keplr-events` (typed pub/sub, rooms per pane kind),
`keplr-client` (the model: panes, tabs, rail, `Document`, `Terminal`).

**The feedback loop exists**: `keplr-native --snapshot` renders any pane to PNG
through the real rcus pipeline, and `--layout` prints every solved box. Both run
in the showcase workflow on a GPU-less runner. This redesign is verified with
those two tools, not by assertion.

---

## 2. Goal and non-goals

**Goal.** A flat, quiet, macOS-native-feeling shell in the spirit of VS Code: one
nav column, no cards, no floating pills, no dead zones, every box filled to its
edges and separated by a single hairline. Motion everywhere, with intent. Themes
that any user can add by dropping a file in, read identically by both clients.

**Non-goals for this spec.** No plugin system, no marketplace, no settings UI
beyond a theme picker, no multi-window, no replacing CodeMirror in the browser
client, no git UI. Those are separate specs.

---

## 3. The renderer grows first — the box model

Every visual goal below is unreachable without this, so it is phase 1.

### 3.1 One quad, one rounded box

Replace the solid pipeline's flat-colour shader with a signed-distance rounded
box. Each quad carries, per vertex: centre, half-extents, corner radius, fill
colour, border width, border colour, shadow offset/blur/spread, shadow colour.

The fragment shader evaluates the rounded-box SDF, antialiases the edge over one
pixel, fills, strokes the border inside the box, and composites the shadow from
the same distance field. One draw call still covers the whole frame; only the
vertex grows. This is the standard technique behind every CSS `border-radius` +
`box-shadow`, and it buys radius, borders and shadows in one change rather than
three features bolted on.

`Style` gains `radius`, `border_width`, `border_color`, and
`shadow: Option<Shadow>`. All default to "nothing", so every existing view is
unchanged. `0` radius and no border must be measurably the same pixels as today.

### 3.2 A clock

`rcus_app::App` gains a frame clock: `Animated<T>` (a value plus a start, an end,
a duration, a curve, and a clock) and `App::advance(now) -> bool`, which returns
whether anything moved. `rcus_desktop` wakes the event loop at display refresh
while `advance` reports motion and sleeps (`ControlFlow::Wait`) the instant it
returns false, so an idle window costs nothing.

Curves, both first-class in the theme: `Ease` over a cubic-bezier control pair,
and `Spring` over stiffness/damping integrated at frame time. Cubic for state
changes, spring for anything a finger moved.

---

## 4. Themes: a file the user drops in

`<root>/.keplr/themes/<anything>.json`. Nothing to install; the picker lists every
file. Built-in themes ship in the same format, so there is exactly one schema and
no privileged themes.

```json
{
  "name": "Keplr Dark",
  "appearance": "dark",
  "ui": {
    "fontSize": 13,
    "lineHeight": 1.5,
    "density": "comfortable",
    "borderWidth": 1,
    "radius": { "sm": 4, "md": 6, "lg": 10 }
  },
  "colors": {
    "chrome":      "#000000",
    "surface":     "#0D0D0F",
    "raised":      "#141417",
    "overlay":     "#1C1C20",
    "border":      "#232327",
    "borderStrong":"#2E2E34",
    "text":        "#E6E6EA",
    "textMuted":   "#8A8A93",
    "textFaint":   "#5A5A63",
    "accent":      "#0A84FF",
    "onAccent":    "#FFFFFF",
    "focusRing":   "#0A84FF",
    "selection":   "#264F78",
    "currentLine": "#FFFFFF0A",
    "indentGuide": "#FFFFFF12",
    "scrollbar":   "#FFFFFF1F",
    "success":     "#30D158",
    "warning":     "#FFD60A",
    "error":       "#FF453A",
    "info":        "#64D2FF"
  },
  "syntax": {
    "keyword": "#BF5AF2", "string": "#FF9F0A", "number": "#30D158",
    "comment": "#5A5A63", "type": "#64D2FF",    "function": "#0A84FF",
    "variable":"#E6E6EA","operator":"#8A8A93",  "constant": "#FF9F0A",
    "macro": "#BF5AF2",   "property": "#64D2FF"
  },
  "motion": {
    "fast": 120, "normal": 180, "slow": 260,
    "ease": [0.2, 0.0, 0.0, 1.0],
    "spring": { "stiffness": 220, "damping": 26 },
    "reducedMotion": false
  }
}
```

Every colour is `#rrggbb` or `#rrggbbaa`. A missing or malformed key falls back to
the default theme's value and is reported in the status bar — a bad theme file
degrades one token, it never blanks the window.

**Both clients read this file.** The native client maps it straight to `Color`.
The browser client turns it into `:root { --k-chrome: #000; … }` at page load,
which is exactly what the existing drift test checks, so the drift test becomes a
round-trip test instead of a hand-maintained CSS block.

---

## 5. The shell

One nav column, one sidebar, one editor area, one panel, one status bar. Panels
are flush and separated by a 1px rule — never a bordered card on a floating
surface.

```
┌──────────────────────────────────────────────────────────────────────┐
│ ●●●  keplr  ~/projects/keplr        ⌘K Search…            ⋯    ⚙     │ 38
├────┬─────────────────┬───────────────────────────────────────────────┤
│ ⌘  │  Files          │ workbench.rs ×   main.rs ×            +  ⋯    │ 35
│ ⌕  │ ──────────────  ├───────────────────────────────────────────────┤
│ ⑂  │  ▾ crates       │  1 use serde::{Deserialize, Serialize};      │
│ ▤  │    ▾ keplr-     │  2                                         │  19
│    │      native     │  3 use std::error::Error;                   │  per
│    │        src      │  4                                         │  line
│    │  ▸ docs         │  5 const N: usize = 4;                      │
├────┴─────────────────┴───────────────────────────────────────────────┤
│ Problems ▪ Terminal ▪ Output                    drag to resize       │ 240
├──────────────────────────────────────────────────────────────────────┤
│ ⑂ main*   ⊗0 ⚠2   Ln 5, Col 24   Rust   Spaces: 4   UTF-8           │ 24
└──────────────────────────────────────────────────────────────────────┘
   48   260 (drag)            flexible              240 (drag)          24
```

Rules that kill what the screenshots show:

- **One** nav row. The activity bar selects the sidebar view; the sidebar does not
  repeat it as a tab row.
- The editor area is **flush**: its tabs sit on the same 1px rule as the sidebar
  and panel, with no inset, no shadow, no rounding.
- **No floating welcome pill.** `⌘K` opens the palette; that is the affordance.
- The editor fills its box to the last pixel. If a file is shorter than the
  viewport, the space below it is editor background, not a dead grey band.
- Files are shown by name with a kind glyph; the file tree keeps one row height
  and one hit target per row.
- Breadcrumbs move **into** the tab row's right side, not onto a second row.

Sizes: title bar 38, tab strip 35, line height 19 at `fontSize` 13, activity bar
48, sidebar 260 default / 200–520 draggable, panel 240 default / 120–70% draggable,
status bar 24. Padding is 8 inside bars, 12 in panes, 20 in the palette.

---

## 6. The editor

The browser client keeps CodeMirror and is restyled to the same spec. The native
editor is new work; `keplr-lang` already provides the language intelligence, so
this is mostly wiring.

Native editor gains, in order: syntax highlighting from `keplr_lang::highlight`
mapped onto the theme's `syntax` tokens; current-line highlight and indent guides;
selection (click, drag, shift-click, `Alt`-click for a second cursor);
undo/redo as a real undo stack with coalescing; find and replace over the visible
range; bracket matching; and a caret that blinks from the clock, not from CSS.

Selection is the load-bearing piece: without it the editor is a viewer. It is
also what the `Animated<T>` clock exists for — the selection rect and the caret
tween between positions instead of teleporting.

---

## 7. Motion

Expressive, as asked, with a floor and a ceiling so it stays a tool.

| What | Curve | Duration |
|---|---|---|
| Panel and sidebar open/close | ease, 8px slide + fade | 180 |
| Palette and menus | spring | — |
| Tab and selection indicators | ease | 120 |
| Hover, press, focus ring | ease, opacity + 1px ring | 120 |
| Resize handles | spring, 1:1 with the pointer | — |
| First paint of a list | stagger, 8ms per row, capped at 12 rows | 180 |
| Progress and long tasks | shimmer, looping | 1400 |
| Window open | ease, scale 0.98→1 + fade | 200 |

Rules: nothing loops except the shimmer; nothing moves that did not change state;
motion is transform/opacity/colour only, never layout; and
`motion.reducedMotion: true`, the macOS "Reduce Motion" setting, or
`prefers-reduced-motion` collapses every duration to 0 while keeping the end
states.

---

## 8. The index page

`keplr-native welcome` and the browser's `?` route render the same content: the
project name and root, a "recent" list, and four actions — open a file, open the
folder, start a terminal, change theme. It fills the editor area when no file is
open, which is what replaces the floating pill. The same theme keys drive it, so
it is a themed view like every other, not a special case.

---

## 9. One system, two renderings

| Concern | Native | Browser |
|---|---|---|
| Layout | rcus `ViewNode` tree | CSS grid/flex |
| Colour | `Color` from the theme file | `:root` custom properties generated from the same file |
| Box model | SDF quad shader | CSS radius/border/box-shadow |
| Motion | `Animated<T>` + display clock | CSS transitions/keyframes, same curves |
| Editor | new, in `keplr-client` | CodeMirror 6, restyled |

The shared contract is the theme file and the layout/motion spec, not the code.
Where the two could drift, the drift test compares the generated custom properties
against the file.

---

## 10. Sequencing

Each phase lands, is screenshotted, and is usable before the next starts.

1. **Box model + clock** (rcus): SDF quads, `Style` additions, `Animated<T>`,
   desktop frame clock. No Keplr changes yet; a `rounded-check` example in rcus
   proves radius, border and shadow with pixel assertions.
2. **Theme system**: schema, discovery, validation, picker; both clients wired;
   the drift test becomes a round-trip test. AMOLED becomes theme #1.
3. **Motion**: curves, springs, reduce-motion, and the animation catalogue above
   applied to the existing native views.
4. **Shell layout**: both clients, per §5.
5. **Native editor**: per §6.
6. **Index page**: per §8.

Phases 1–3 are rcus and small. Phase 4 is the largest. Phases 5–6 are independent
of each other.

---

## 11. How each phase is verified

- `keplr-native --snapshot` renders every pane; the showcase workflow uploads the
  PNGs and `layout.txt`. A layout regression is a number in `layout.txt`, not an
  opinion about a dark image.
- `rcus/examples/rounded-check` asserts pixel values: the centre of a rounded
  corner is background, the centre of a border pixel is the border colour, and a
  shadow at a known offset is darker than the surface but lighter than chrome.
- Theme parsing gets unit tests for every failure mode: missing file, malformed
  JSON, unknown key, bad hex, out-of-range duration.
- The layout gets a headless test per rule in §5 — one nav row, bars flush to the
  window edges, no gap between the editor and the status bar.
- Both clients keep their existing suites; nothing is deleted to make a test pass.

---

## 12. Risks, stated plainly

- **Phase 1 changes every pixel rcus draws.** The SDF shader must reproduce the
  current flat rect exactly when radius and border are zero, or every existing
  view shifts. That is the first thing the pixel test pins.
- **The browser client is 6445 lines of hand-written HTML.** Restyling it is
  mechanical but large; it is the most likely source of regressions and the
  slowest part of phase 4.
- **A spring clock is always-on code.** Idle cost is bounded by sleeping on
  `advance() == false`, but this is new machinery in the render loop and the
  place a bug would be hardest to see.
- **Two editors will never be identical.** The browser gets CodeMirror's
  accessibility and IME handling for free; the native one is built from scratch.
  I am not going to claim parity in this spec.