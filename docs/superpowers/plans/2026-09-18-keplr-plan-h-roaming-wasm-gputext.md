# Keplr Plan H — WASM Surgery, GPU Text, Roaming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the last three structural gaps: whole-workspace WASM check-cleanliness, GPU text rendering in the native shell, and a real roaming sync channel with a CLI client.

**Architecture:** WASM is done by target-gated deps plus `#[cfg]` twins/gates so every crate keeps its native behavior byte-identical while `wasm32-unknown-unknown` compiles; pure logic (fuzzy, CRDT memory ops, scene types, UI state, snippets data, fingerprints of nothing) stays live on both targets, OS-bound work degrades honestly. GPU text adds a second textured pipeline fed by the existing atlas, shaping editor lines with highlight colors. Roaming is an Axum WebSocket hub (`/sync/channel`) over the existing `SyncDoc` registry plus a `keplr sync` client on `tokio-tungstenite`, reusing the bearer gate (query token works for WS).

**Tech Stack:** Rust 1.97.1, existing workspace deps plus `tokio-tungstenite 0.26`, `futures-util 0.3`, `http 1` (CLI client only).

**Spec:** `docs/superpowers/specs/2026-09-17-keplr-design.md` sections 2 (WASM parity, native shell), 4 (editor), 5 (roaming)

## Global Constraints

- No Electron, no Chromium shell, no WebView. Native binary + `keplr serve` JSON only.
- Same types with no rename, no signature changes on any existing public item. New items only.
- RAM soft cap 500MB respected everywhere; new walks keep existing caps.
- Git remains truth; derived state under `{workspace}/.keplr/` (gitignored).
- LAML reuse only.
- Every part ends with a local commit; the whole plan ends with push + `gh run watch` green (default job + `gpu-check` + `wasm-check`).
- No placeholders: wasm twins report honestly (`vec![]`, `bail!`, documented no-ops); GPU-less paths fall back; WS failures close cleanly.
- Standing orders for this plan: no local `cargo` runs (verify via GitHub CI with `gh`), no new test files — existing CI tests must stay green. The compiler is the guide for HW: each `cargo check --target` round names the next ungated use.

---

## File Structure

```
crates/keplr-core/src/lib.rs    # HW: target-gated deps + cfg twins/gates
crates/keplr-core/Cargo.toml    # HW: move notify/ignore/walkdir/tokio to target deps
crates/keplr-sync/src/lib.rs    # HW: gate fs/process fns
crates/keplr-build/src/lib.rs   # HW: gate runners/fingerprints/journal-io
crates/keplr-build/Cargo.toml   # HW: gate ignore/blake3/tokio
crates/keplr-render/src/lib.rs  # HW: gate build_scene/branch_for/fonts
crates/keplr-ui/src/lib.rs      # HW: gate to_scene/run_tasks/branch_name
crates/keplr-render/src/gpu.rs  # HX: text pipeline + layout + integration
crates/keplr-serve/src/lib.rs   # HR: /sync/channel hub
crates/keplr-cli/src/main.rs    # HR: Sync command
crates/keplr-cli/Cargo.toml     # HR: + tokio-tungstenite, futures-util, http
Cargo.toml                      # HR: + tokio-tungstenite, futures-util, http
.github/workflows/ci.yml        # HW: wasm job checks 6 crates
README.md                       # usage per part
```

---

## Part HW: WASM surgery

Rule for every twin: native behavior byte-identical; wasm side honest (`vec![]`, `Self::default()`, `bail!("... on wasm")`, documented no-op). Functions with NO wasm-kept callers get gated whole with no twin.

### Task HW-1: Core gating

**Files:**
- Modify: `crates/keplr-core/Cargo.toml`
- Modify: `crates/keplr-core/src/lib.rs`

- [ ] **Step 1: Cargo.** Old:

```toml
[dependencies]
anyhow.workspace = true
ignore.workspace = true
walkdir.workspace = true
notify.workspace = true
blake3.workspace = true
ropey.workspace = true
nucleo-matcher.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
keplr-sync = { path = "../keplr-sync" }
```

New:

```toml
[dependencies]
anyhow.workspace = true
blake3.workspace = true
ropey.workspace = true
nucleo-matcher.workspace = true
serde.workspace = true
serde_json.workspace = true
keplr-sync = { path = "../keplr-sync" }

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
ignore.workspace = true
walkdir.workspace = true
notify.workspace = true
tokio.workspace = true
```

- [ ] **Step 2: Gates in lib.rs.** Apply exactly:

1. `pub mod git;` → `#[cfg(not(target_arch = "wasm32"))] pub mod git;`
2. `ensure_dirs` → gate whole: `#[cfg(not(target_arch = "wasm32"))] pub fn ensure_dirs...` (keep body).
3. `walk_files` → keep native body under `#[cfg(not(...))]`, add twin:
```rust
    #[cfg(target_arch = "wasm32")]
    pub fn walk_files(&self, _limit: usize) -> Vec<FileEntry> {
        Vec::new()
    }
```
4. `grep`, `grep_trigram` → same treatment (twins return `Vec::new()`).
5. `Index`: keep struct + `files/len/is_empty/get`; gate `build/load/save/apply/refresh/grep/index_path`. Twins:
   - `build` → `Self::default()`
   - `load` → `Self::default()` (signature `(_ws: &Workspace)`)
   - `save` → `bail!("no filesystem on wasm")`
   - `apply` → empty body `{}` with `(_ws, _path)` names
   - `refresh` → `Vec::new()`
   - `grep` → `Vec::new()`
6. `poll_changes` (+ `Change/ChangeKind` types stay) → twin `bail!("notify unavailable on wasm")`.
7. `TrigramIndex`: struct + `candidates` stay; `build` → twin `Self::default()` with `(_ws, _cap_files, _cap_bytes)` names.
8. `save_buffer`, `git_commit_file` → gate whole, no twins.
9. `synth_tree` → gate whole, no twin. `percentile_ns`, `lcg_next` stay.

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-core/Cargo.toml crates/keplr-core/src/lib.rs
git commit -m "feat(wasm): gate core os-bound work with honest twins"
```

### Task HW-2: Sync gating

**Files:**
- Modify: `crates/keplr-sync/src/lib.rs` (no Cargo change unless CI demands; `yrs` stays ungated on the first attempt)

- [ ] **Step 1: Gates.**
1. `Cas::new`, `path_for` stay. `put` → twin `bail!("no filesystem on wasm")`; `get` → twin bail; `exists` → twin `false`.
2. `save_snapshot`, `load_snapshot` → twins bail. `restore_snapshot` stays (pure). `snapshot_name_ok` stays.
3. `ensure_materialized`, `lfs_pointer_of_file` → gate whole, no twins.
4. `parse_lfs_pointer`, `is_lfs_pointer_text`, `LfsPointer`, `SyncDoc` (+all methods) stay ungated on the first attempt.

If CI reports `yrs` itself failing on wasm, fall back: move `yrs.workspace = true` into a `target.'cfg(not(...))'` block and gate `SyncDoc` + `restore_snapshot` whole. (Implementer: only if the compiler demands it.)

- [ ] **Step 2: Commit locally**

```bash
git add crates/keplr-sync/src/lib.rs crates/keplr-sync/Cargo.toml
git commit -m "feat(wasm): gate sync fs and process edges"
```

### Task HW-3: Build gating

**Files:**
- Modify: `crates/keplr-build/Cargo.toml`
- Modify: `crates/keplr-build/src/lib.rs`

- [ ] **Step 1: Cargo.** Read it first (has anyhow/serde/serde_json/blake3/ignore/keplr-sync; tokio may or may not be listed — gate `ignore`, `blake3`, and `tokio` iff present):

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
ignore.workspace = true
blake3.workspace = true
```

(+ `tokio.workspace = true` in that block iff the `[dependencies]` table lists it; remove it from `[dependencies]` in that case.)

- [ ] **Step 2: Gates.**
1. `load_tasks` → twin `bail!("no filesystem on wasm")`.
2. `run_task` → twin `bail!("no processes on wasm")`.
3. `hash_file`, `hash_listed`, `task_fingerprint`, `fingerprint_inner`, `graph_fingerprints`, `store_outputs`, `restore_outputs`, `run_ordered`, `run_graph`, `run_graph_parallel`, `run_graph_settled`, `cancelled_entries`, `level_index`, `tail_2k` → gate whole, no twins.
4. `load_journal` → twin `Self`... it returns a map: twin `BTreeMap::new()`. `save_journal` → twin `Ok(())` documented as wasm no-op.
5. Keep: `TaskDef/RawTask/FileShape/JournalEntry/RunReport` types, `topo_levels/topo_order/closure`.

- [ ] **Step 3: Commit locally**

```bash
git add crates/keplr-build/Cargo.toml crates/keplr-build/src/lib.rs
git commit -m "feat(wasm): gate build runners keeping pure dag logic"
```

### Task HW-4: Render + UI gating + CI job

**Files:**
- Modify: `crates/keplr-render/src/lib.rs`
- Modify: `crates/keplr-ui/src/lib.rs`
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Render gates.** Gate whole (no twins): `build_scene`, `branch_for`, `dirs_home`, `font_stack`, `discover_font`. Everything else (Theme/Rect/TitleBar/Panel/DockPane/TaskEntry/BottomPane/Squiggle/EditorPane/StatusBar/Scene/SceneOp/PALETTE_COMMANDS/filter_commands/breadcrumbs_for/SceneSpec/lang_label/truncate/push_op/diff_scenes/PaintBackend/AnsiBackend) stays.

- [ ] **Step 2: UI gates.** Gate whole (no twins): `to_scene`, `run_tasks`, `branch_name`. `palette_results`: keep, but the files branch must not call anything missing on wasm — `walk_files` has a twin returning empty, so it compiles; add a doc comment noting files mode yields empty on wasm. Everything else stays.

- [ ] **Step 3: CI job.** Old wasm job:

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

New last line:

```yaml
      - run: cargo check -p keplr-lang -p keplr-sync -p keplr-build -p keplr-core -p keplr-render -p keplr-ui --target wasm32-unknown-unknown
```

- [ ] **Step 4: Commit locally**

```bash
git add crates/keplr-render/src/lib.rs crates/keplr-ui/src/lib.rs .github/workflows/ci.yml
git commit -m "feat(wasm): gate scene builders keeping pure ui types"
```

Then run the compiler loop: push is deferred to the end of the whole plan, but HW MUST be verified before HX/HR land (they touch the same files). Compromise (locked): push after HW-4, watch only `wasm-check` + `build-test` via `gh run view --json`, fix forward, then continue HX/HR on top. The final push at plan end re-verifies everything.

---

## Part HX: GPU text layout

### Task HX-1: Text pipeline + shaping + App integration

**Files:**
- Modify: `crates/keplr-render/src/gpu.rs` (append text stack + wire into `Gpu`/`App`/frame)

- [ ] **Step 1: Append** (end of file):

```rust
fn token_rgb(kind: keplr_lang::TokenKind) -> [f32; 3] {
    match kind {
        keplr_lang::TokenKind::Keyword => [0.35, 0.65, 1.0],
        keplr_lang::TokenKind::Str => [0.45, 0.85, 0.55],
        keplr_lang::TokenKind::Comment => [0.55, 0.55, 0.6],
        keplr_lang::TokenKind::Number => [0.95, 0.75, 0.35],
        keplr_lang::TokenKind::Other => [0.9, 0.93, 0.95],
    }
}

fn lang_of(label: &str) -> keplr_lang::LangKind {
    keplr_lang::LangKind::from_path(Path::new(&format!("x.{label}")))
}

const TEXT_SHADER: &str = r#"
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) col: vec4<f32>,
};
@vertex
fn vs(@location(0) p: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) c: vec4<f32>) -> VsOut {
    var o: VsOut;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.uv = uv;
    o.col = c;
    return o;
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let a = textureSample(t, s, in.uv).r;
    return vec4<f32>(in.col.rgb, in.col.a * a);
}
"#;

pub fn layout_colored_line(
    atlas: &GlyphAtlas,
    line: &str,
    spans: &[keplr_lang::Span],
    x_px: f32,
    y_top_px: f32,
    w_px: f32,
    h_px: f32,
) -> Vec<f32> {
    let mut v = Vec::new();
    let nx = |px: f32| px / w_px * 2.0 - 1.0;
    let ny = |py: f32| 1.0 - py / h_px * 2.0;
    let mut pen = x_px;
    let use_spans = if spans.is_empty() { false } else { true };
    let fallback = [keplr_lang::Span {
        start: 0,
        len: line.len(),
        kind: keplr_lang::TokenKind::Other,
    }];
    let spans = if use_spans { spans } else { &fallback[..] };
    for s in spans {
        let col = token_rgb(s.kind);
        let piece = byte_slice_span(line, s.start, s.len);
        for c in piece.chars() {
            let key = (c, 16u32);
            let spot = match atlas.glyphs.get(&key) {
                Some(g) => *g,
                None => {
                    if let Some(space) = atlas.glyphs.get(&(' ', 16u32)) {
                        pen += space.advance;
                    }
                    continue;
                }
            };
            if spot.w > 0 && spot.h > 0 {
                let gx0 = pen + spot.bx;
                let gy0 = y_top_px + spot.by;
                let gx1 = gx0 + spot.w as f32;
                let gy1 = gy0 + spot.h as f32;
                let u0 = spot.x as f32 / atlas.width as f32;
                let v0 = spot.y as f32 / atlas.height as f32;
                let u1 = (spot.x + spot.w) as f32 / atlas.width as f32;
                let v1 = (spot.y + spot.h) as f32 / atlas.height as f32;
                let quad = [
                    (nx(gx0), ny(gy0), u0, v0),
                    (nx(gx1), ny(gy0), u1, v0),
                    (nx(gx1), ny(gy1), u1, v1),
                    (nx(gx0), ny(gy0), u0, v0),
                    (nx(gx1), ny(gy1), u1, v1),
                    (nx(gx0), ny(gy1), u0, v1),
                ];
                for (px, py, u, vv) in quad {
                    v.push(px);
                    v.push(py);
                    v.push(u);
                    v.push(vv);
                    v.push(col[0]);
                    v.push(col[1]);
                    v.push(col[2]);
                    v.push(1.0);
                }
            }
            pen += spot.advance;
        }
    }
    v
}

fn byte_slice_span(text: &str, start: usize, len: usize) -> &str {
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

pub fn scene_text_quads(
    atlas: &GlyphAtlas,
    scene: &Scene,
    w_px: u32,
    h_px: u32,
) -> Vec<f32> {
    let lang = lang_of(&scene.center.lang);
    let mut v = Vec::new();
    let x0 = w_px as f32 * 0.22 + 12.0;
    let mut y = 44.0f32;
    for line in scene.center.lines.iter().take(25) {
        let spans = keplr_lang::highlight(lang, line);
        v.extend(layout_colored_line(
            atlas,
            line,
            &spans,
            x0,
            y,
            w_px as f32,
            h_px as f32,
        ));
        y += 18.0;
    }
    v
}
```

Atlas is built at 16px (`id = px as u32` keyed); layout uses the `(c, 16u32)` key — both sides agree on 16. `Path` import: gpu.rs needs `use std::path::Path;` — check current imports (it has `PathBuf`); extend.

- [ ] **Step 2: Text GPU objects.** Extend `Gpu` struct + `new` + add upload helper usage:

Struct gains:

```rust
    text_pipeline: wgpu::RenderPipeline,
    text_bind_group: wgpu::BindGroup,
    text_vbuf: wgpu::Buffer,
    text_vcap: usize,
    text_on: bool,
```

In `Gpu::new`, after the layout pipeline creation, build the text stack from the discovered font:

```rust
        let (text_pipeline, text_bind_group, text_vbuf, text_on) =
            match super::discover_font()
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|bytes| build_atlas(&bytes, 16.0).ok())
            {
                Some(atlas) => {
                    let texture = upload_atlas(&device, &queue, &atlas);
                    let view =
                        texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let sampler =
                        device.create_sampler(&wgpu::SamplerDescriptor::default());
                    let bgl =
                        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: None,
                            entries: &[
                                wgpu::BindGroupLayoutEntry {
                                    binding: 0,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Texture {
                                        sample_type: wgpu::TextureSampleType::Float {
                                            filterable: true,
                                        },
                                        view_dimension: wgpu::TextureViewDimension::D2,
                                        multisampled: false,
                                    },
                                    count: None,
                                },
                                wgpu::BindGroupLayoutEntry {
                                    binding: 1,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Sampler(
                                        wgpu::SamplerBindingType::Filtering,
                                    ),
                                    count: None,
                                },
                            ],
                        });
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: None,
                        layout: &bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&sampler),
                            },
                        ],
                    });
                    let tshader =
                        device.create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: Some("keplr-text"),
                            source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
                        });
                    let tlayout =
                        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                            label: None,
                            bind_group_layouts: &[&bgl],
                            push_constant_ranges: &[],
                        });
                    let tpipeline =
                        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                            label: None,
                            layout: Some(&tlayout),
                            vertex: wgpu::VertexState {
                                module: &tshader,
                                entry_point: "vs",
                                compilation_options:
                                    wgpu::PipelineCompilationOptions::default(),
                                buffers: &[wgpu::VertexBufferLayout {
                                    array_stride: 32,
                                    step_mode: wgpu::VertexStepMode::Vertex,
                                    attributes: &[
                                        wgpu::VertexAttribute {
                                            format: wgpu::VertexFormat::Float32x2,
                                            offset: 0,
                                            shader_location: 0,
                                        },
                                        wgpu::VertexAttribute {
                                            format: wgpu::VertexFormat::Float32x2,
                                            offset: 8,
                                            shader_location: 1,
                                        },
                                        wgpu::VertexAttribute {
                                            format: wgpu::VertexFormat::Float32x4,
                                            offset: 16,
                                            shader_location: 2,
                                        },
                                    ],
                                }],
                            },
                            fragment: Some(wgpu::FragmentState {
                                module: &tshader,
                                entry_point: "fs",
                                compilation_options:
                                    wgpu::PipelineCompilationOptions::default(),
                                targets: &[Some(wgpu::ColorTargetState {
                                    format,
                                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                                    write_mask: wgpu::ColorWrites::ALL,
                                })],
                            }),
                            primitive: wgpu::PrimitiveState::default(),
                            depth_stencil: None,
                            multisample: wgpu::MultisampleState::default(),
                            multiview: None,
                            cache: None,
                        });
                    let tvbuf = device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: 65536,
                        usage: wgpu::BufferUsages::VERTEX
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    (tpipeline, bg, tvbuf, true)
                }
                None => {
                    let tshader =
                        device.create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: Some("keplr-text-off"),
                            source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
                        });
                    let tlayout =
                        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                            label: None,
                            bind_group_layouts: &[],
                            push_constant_ranges: &[],
                        });
                    let tpipeline =
                        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                            label: None,
                            layout: Some(&tlayout),
                            vertex: wgpu::VertexState {
                                module: &tshader,
                                entry_point: "vs",
                                compilation_options:
                                    wgpu::PipelineCompilationOptions::default(),
                                buffers: &[],
                            },
                            fragment: None,
                            primitive: wgpu::PrimitiveState::default(),
                            depth_stencil: None,
                            multisample: wgpu::MultisampleState::default(),
                            multiview: None,
                            cache: None,
                        });
                    let tvbuf = device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: 16,
                        usage: wgpu::BufferUsages::VERTEX
                            | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: None,
                        layout: &device.create_bind_group_layout(
                            &wgpu::BindGroupLayoutDescriptor {
                                label: None,
                                entries: &[],
                            },
                        ),
                        entries: &[],
                    });
                    (tpipeline, bg, tvbuf, false)
                }
            };
```

Hmm — the None branch builds a broken pipeline (vertex with no buffers but shader expects attributes). If text is off we never draw with it, but creating a pipeline whose layout has no bind groups while the shader declares group(0) bindings is fine at creation (only use-time mismatch matters, and we never bind/draw when off). Vertex buffers `&[]` with a shader reading locations is also creation-legal. OK. But wait — simpler: make text fields `Option<>`? That ripples through frame(). The degraded-pipeline approach keeps frame() branchless except `if text_on`. Keep as written. Actually — clippy may flag the duplicated pipeline creation as too much duplication? No such default lint for blocks. Fine.

`format`/`view`/`sampler` moves: `format` is Copy (enum) ✓ used later for the layout pipeline? Order in `new`: layout pipeline is created BEFORE this block in the current code... The text block needs `format` (Copy ✓), device/queue (borrowed ✓). Insert the text block right after `let pipeline = ...` creation, before `let vbuf`. It references `build_atlas`, `upload_atlas`, `super::discover_font` — all exist. `view`/`sampler` locals consumed by bg ✓.

- [ ] **Step 3: Frame integration.** In `frame()`, add a `text: Option<(&BindGroup-ish…)}`... cleaner: second method `frame_text(&mut self, verts: &[f32])`:

```rust
    fn frame_text(&mut self, view: &wgpu::TextureView, verts: &[f32]) {
        if !self.text_on || verts.is_empty() {
            return;
        }
        let bytes = f32_to_bytes(verts);
        if bytes.len() > self.text_vcap {
            self.text_vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.text_vcap = bytes.len();
        }
        self.queue.write_buffer(&self.text_vbuf, 0, &bytes);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.text_pipeline);
            pass.set_bind_group(0, &self.text_bind_group, &[]);
            pass.set_vertex_buffer(0, self.text_vbuf.slice(..));
            pass.draw(0..verts.len() as u32 / 8, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }
```

`frame()` must hand its view over: restructure `frame()` to create the view, run the layout pass, then return the view? Views borrow the texture which is method-local... Restructure `frame(clear, verts)` → keep as-is, and add text drawing INSIDE `frame` by taking `text_verts: &[f32]` param? Signature change is internal (only App calls it). New signature: `fn frame(&mut self, clear: wgpu::Color, verts: usize, text_verts: &[f32]) -> anyhow::Result<()>` doing layout pass then (if text_on && !empty) upload + text pass on the same view, single submit of two encoders (or one encoder, two passes — use one encoder, two `begin_render_pass` sequentially; second with Load). Implementer: rewrite `frame` accordingly (upload inline, no separate method — drop `frame_text`, fold in).

Concretely the new `frame`:

```rust
    fn frame(
        &mut self,
        clear: wgpu::Color,
        verts: usize,
        text_verts: &[f32],
    ) -> anyhow::Result<()> {
        let texture = self
            .surface
            .get_current_texture()
            .map_err(|e| anyhow::anyhow!("surface lost: {e:?}"))?;
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                ... layout pass as today, LoadOp::Clear ...
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            pass.draw(0..verts as u32, 0..1);
        }
        if self.text_on && !text_verts.is_empty() {
            let bytes = f32_to_bytes(text_verts);
            if bytes.len() > self.text_vcap {
                self.text_vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.text_vcap = bytes.len();
            }
            self.queue.write_buffer(&self.text_vbuf, 0, &bytes);
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.text_pipeline);
                pass.set_bind_group(0, &self.text_bind_group, &[]);
                pass.set_vertex_buffer(0, self.text_vbuf.slice(..));
                pass.draw(0..text_verts.len() as u32 / 8, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        texture.present();
        Ok(())
    }
```

Borrow check: `self.text_vbuf` reassigned then `self.queue.write_buffer(&self.text_vbuf, ...)` — field-disjoint borrows through same `self` in one function: `self.text_vbuf = ...` (mut borrow of field) then `self.queue.write_buffer(&self.text_vbuf)` — sequential, NLL fine. But inside `if self.text_on` — `self.text_on` is Copy bool read ✓.

App call site: needs the atlas to shape text. Store `atlas: Option<GlyphAtlas>`? Atlas pixels needed only at upload (done in new). Shaping needs `glyphs` map + width/height. Store `atlas: Option<GlyphAtlas>` in App? GlyphAtlas has no wgpu types ✓ easy. In `new`, return atlas too... `Gpu::new` returns Self; stash `text_atlas: Option<GlyphAtlas>` field on Gpu (build alongside). Then RedrawRequested:

```rust
let text_verts = match (&gpu.text_atlas, &self.scene) {
    (Some(atlas), Some(scene)) => {
        scene_text_quads(atlas, scene, gpu.size.0, gpu.size.1)
    }
    _ => Vec::new(),
};
...gpu.frame(clear, n, &text_verts)...
```

So `Gpu` gains `text_atlas: Option<GlyphAtlas>` set in the Some branch (`Some(atlas)` — atlas moved into upload? `upload_atlas(&device, &queue, &atlas)` borrows ✓ then store). In None branch: `text_atlas: None`.

Update the call `gpu.frame(clear, n)` → `gpu.frame(clear, n, &text_verts)`.

- [ ] **Step 4: Commit locally**

```bash
git add crates/keplr-render/src/gpu.rs
git commit -m "feat(gpu): textured text pipeline shaping editor lines"
```

---

## Part HR: Roaming channel + client

### Task HR-1: Server hub + client deps

**Files:**
- Modify: `Cargo.toml` (+ tokio-tungstenite, futures-util, http)
- Modify: `crates/keplr-cli/Cargo.toml` (+ the three)
- Modify: `crates/keplr-serve/src/lib.rs` (registry state + `/sync/channel`)

**Interfaces:**
- Produces: `GET /sync/channel?name=` (WebSocket, bearer-compatible via query token), `keplr sync --url URL [--name N] [--file F] [--token T] [--once]`

- [ ] **Step 1: Deps.** Workspace append:

```toml
tokio-tungstenite = "0.26"
futures-util = "0.3"
http = "1"
```

CLI append:

```toml
tokio-tungstenite.workspace = true
futures-util.workspace = true
http.workspace = true
```

- [ ] **Step 2: Serve hub.** AppState gains:

```rust
    sync_docs: Arc<Mutex<HashMap<String, keplr_sync::SyncDoc>>>,
    sync_tx: Arc<Mutex<HashMap<String, tokio::sync::broadcast::Sender<Vec<u8>>>>>,
```

Update both constructors (`serve_with_token` was replaced by `serve_full` — find `let state = AppState {` in `serve_full`; there is exactly one). Add the two fields with empty maps.

Handlers (append before `pub async fn serve`):

```rust
async fn sync_channel(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> impl axum::response::IntoResponse {
    let name = params
        .get("name")
        .cloned()
        .unwrap_or_else(|| String::from("buffer"));
    ws.on_upgrade(move |socket| channel_loop(state, name, socket))
}

async fn channel_loop(
    state: AppState,
    name: String,
    mut socket: axum::extract::ws::WebSocket,
) {
    use axum::extract::ws::Message;
    let (full, mut rx) = {
        let mut docs = state.sync_docs.lock().unwrap_or_else(|e| e.into_inner());
        let doc = docs
            .entry(name.clone())
            .or_insert_with(|| keplr_sync::SyncDoc::new(&name));
        let full = doc.encode_update();
        let mut txs = state.sync_tx.lock().unwrap_or_else(|e| e.into_inner());
        let tx = txs
            .entry(name.clone())
            .or_insert_with(|| tokio::sync::broadcast::channel(64).0)
            .clone();
        (full, tx.subscribe())
    };
    if socket.send(Message::Binary(full)).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        let mut docs = state.sync_docs.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(doc) = docs.get(&name) {
                            if doc.apply_update(&bytes).is_ok() {
                                let merged = doc.encode_update();
                                drop(docs);
                                let txs = state.sync_tx.lock().unwrap_or_else(|e| e.into_inner());
                                if let Some(tx) = txs.get(&name) {
                                    let _ = tx.send(merged);
                                }
                            }
                        }
                    }
                    _ => break,
                }
            }
            update = rx.recv() => {
                match update {
                    Ok(bytes) => {
                        if socket.send(Message::Binary(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }
}
```

Self-echo note: the sender also receives its own broadcast; CRDT apply is idempotent and the client never re-sends on receive, so no loop. Document in code comment (add one line above the broadcast send: `// echo is idempotent; clients must not re-send on receive`).

Register `.route("/sync/channel", get(sync_channel))`.

`SyncDoc: Send` holds across the Mutex: `HashMap<String, SyncDoc>` in `Arc<Mutex<>>` requires `SyncDoc: Send` — yrs `Doc` is `Send`. If CI disagrees, wrap per-doc `Arc<Mutex<SyncDoc>>`... preemptively? No — try direct first.

- [ ] **Step 3: Commit locally**

```bash
git add Cargo.toml crates/keplr-cli/Cargo.toml crates/keplr-serve/src/lib.rs
git commit -m "feat(sync): websocket roaming hub with doc registry"
```

### Task HR-2: `keplr sync` client + docs + verify

**Files:**
- Modify: `crates/keplr-cli/src/main.rs` (`Sync` variant + arm)
- Modify: `README.md` (Plan H link + usage)

- [ ] **Step 1: Variant** (after the `Token` variant — read it; `Token { save, rotate }`):

```rust
    Sync {
        url: String,
        #[arg(long, default_value = "buffer")]
        name: String,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value = "")]
        token: String,
        #[arg(long)]
        once: bool,
    },
```

- [ ] **Step 2: Arm** (after the `Token` arm):

```rust
        Cmd::Sync {
            url,
            name,
            file,
            token,
            once,
        } => {
            use futures_util::{SinkExt, StreamExt};
            let mut target = url.clone();
            if !target.contains('?') {
                target.push_str(&format!("?name={name}"));
            }
            if !token.is_empty() {
                target.push_str(&format!("&token={token}"));
            }
            let req = http::Request::builder()
                .uri(target)
                .header("Authorization", format!("Bearer {token}"))
                .body(())?;
            let (stream, _) = tokio_tungstenite::connect_async(req).await?;
            let (mut sink, mut source) = stream.split();
            let doc = keplr_sync::SyncDoc::new(&name);
            if let Some(f) = &file {
                let seed = std::fs::read_to_string(f).unwrap_or_default();
                doc.push(&seed);
            }
            sink.send(tokio_tungstenite::tungstenite::Message::Binary(
                doc.encode_update(),
            ))
            .await?;
            let mut last_write = file
                .as_ref()
                .and_then(|f| std::fs::metadata(f).ok())
                .and_then(|m| m.modified().ok());
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    msg = source.next() => {
                        match msg {
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(bytes))) => {
                                doc.apply_update(&bytes)?;
                                if let Some(f) = &file {
                                    std::fs::write(f, doc.content())?;
                                    last_write = std::fs::metadata(f).ok().and_then(|m| m.modified().ok());
                                    println!("sync: received {} bytes, wrote {}", bytes.len(), f.display());
                                } else {
                                    println!("sync: received {} bytes", bytes.len());
                                }
                                if once {
                                    return Ok(());
                                }
                            }
                            Some(Ok(_)) => {}
                            _ => {
                                eprintln!("sync: channel closed");
                                return Ok(());
                            }
                        }
                    }
                    _ = tick.tick() => {
                        if once {
                            continue;
                        }
                        if let Some(f) = &file {
                            let changed = std::fs::metadata(f)
                                .ok()
                                .and_then(|m| m.modified().ok())
                                .map(|t| Some(t) != last_write)
                                .unwrap_or(false);
                            if changed {
                                let text = std::fs::read_to_string(f).unwrap_or_default();
                                if text != doc.content() {
                                    let len = doc.content().chars().count() as u32;
                                    {
                                        // replace-all via fresh ops: remove then push
                                        // (SyncDoc has no replace; emulate with remove_range if present,
                                        // else rebuild doc state by re-seeding a sibling update)
                                    }
                                    let _ = len;
                                    // simplest correct bridge: fresh doc carries the file
                                    let fresh = keplr_sync::SyncDoc::from_text(&name, &text);
                                    let update = fresh.encode_update();
                                    doc.apply_update(&update)?;
                                    sink.send(tokio_tungstenite::tungstenite::Message::Binary(update)).await?;
                                    println!("sync: sent file change");
                                }
                                last_write = std::fs::metadata(f).ok().and_then(|m| m.modified().ok());
                            }
                        }
                    }
                }
            }
        }
```

Clean up before writing: the dead `let len`/`let _ = len` block and empty comment must go — replace with just the fresh-doc bridge (decided design). Final tick body:

```rust
                            if changed {
                                let text = std::fs::read_to_string(f).unwrap_or_default();
                                if text != doc.content() {
                                    let fresh = keplr_sync::SyncDoc::from_text(&name, &text);
                                    let update = fresh.encode_update();
                                    doc.apply_update(&update)?;
                                    sink.send(tokio_tungstenite::tungstenite::Message::Binary(update)).await?;
                                    println!("sync: sent file change");
                                }
                                last_write = std::fs::metadata(f).ok().and_then(|m| m.modified().ok());
                            }
```

(Implementer: write this clean version, not the draft with the dead block. Semantics: replace-based file bridge; concurrent same-file edits merge at op level when both sides run live — documented here and in README.)

`Duration` import: main.rs has no `std::time::Duration` import — the tick uses the full path `tokio::time::interval(...)` and `MissedTickBehavior`; `Duration` itself isn't named. Check: `tokio::time::interval(Duration::from_millis(500))` — needs `Duration` in scope! Use full path `std::time::Duration::from_millis(500)`. (Implementer: use the full path.)

- [ ] **Step 3: README.** Add `Plan H roaming/wasm/gpu-text: \`docs/superpowers/plans/2026-09-18-keplr-plan-h-roaming-wasm-gputext.md\`` next to the Plan G link, and append:

```markdown
## Use (Plan H roaming/wasm/gpu-text, production)

cargo run -p keplr-cli -- --root . serve --port 7137 &
cargo run -p keplr-cli -- --root /tmp/roam sync --url ws://127.0.0.1:7137/sync/channel --name notes --file notes.txt --once
cargo run -p keplr-cli --features desktop -- --root . desktop --open Cargo.toml  # editor text now GPU-painted
```

- [ ] **Step 4: Commit locally, push, watch all three jobs**

```bash
git add crates/keplr-cli/src/main.rs README.md
git commit -m "feat(sync): roaming client with file bridge"
git push origin main
gh run watch $(gh run list --limit 1 --json databaseId --jq '.[0].databaseId') --interval 15
```

Expected: green. Fix forward from `gh run view <id> --log-failed` (likely candidates: wasm gate misses enumerated by the compiler, `http::Request` client-request impl, `remove_range`-style API drift — none used in final code, `text_vcap` bookkeeping).

---

## Self-review (run before handoff)

1. Spec coverage: §2 WASM parity yes for all six lib crates (CI-verified; axum/CLI stay native — the browser runs UI+CRDT, the hub stays a binary), native text yes (atlas → pipeline → editor quads); §4 editor yes (unchanged TUI); §5 roaming yes (WS hub + file-bridge client, bearer-compatible); per-keystroke streaming now rides the channel (replaces the deferred item).
2. Placeholder scan: no TBD/TODO/placeholder/unimplemented; wasm twins honest; GPU-less desktop falls back; closed channels exit 0.
3. Type consistency: no signatures changed; new items `sync_docs/sync_tx`, `sync_channel/channel_loop`, `Sync` cmd, `layout_colored_line/scene_text_quads`, gated twins spelled identically everywhere.
