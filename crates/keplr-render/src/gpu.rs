use super::{branch_for, Scene, SceneSpec, Theme};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) col: vec4<f32>,
};
@vertex
fn vs(@location(0) p: vec2<f32>, @location(1) c: vec4<f32>) -> VsOut {
    var o: VsOut;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.col = c;
    return o;
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return in.col;
}
"#;

fn parse_hex(hex: &str) -> [f32; 3] {
    let h = hex.trim_start_matches('#');
    let n = u32::from_str_radix(h, 16).unwrap_or(0x0e1116);
    [
        ((n >> 16) & 0xff) as f32 / 255.0,
        ((n >> 8) & 0xff) as f32 / 255.0,
        (n & 0xff) as f32 / 255.0,
    ]
}

fn push_quad(v: &mut Vec<f32>, x: f32, y: f32, w: f32, h: f32, c: [f32; 3]) {
    let corners = [
        (x, y),
        (x + w, y),
        (x + w, y + h),
        (x, y),
        (x + w, y + h),
        (x, y + h),
    ];
    for (px, py) in corners {
        v.push(px);
        v.push(py);
        v.push(c[0]);
        v.push(c[1]);
        v.push(c[2]);
        v.push(1.0);
    }
}

fn layout_quads(_scene: &Scene, w: u32, h: u32) -> Vec<f32> {
    let theme = Theme::zed_dark();
    let bg = parse_hex(&theme.bg);
    let surface = parse_hex(&theme.surface);
    let accent = parse_hex(&theme.accent);
    let w = w.max(1) as f32;
    let h = h.max(1) as f32;
    let nx = |px: f32| px / w * 2.0 - 1.0;
    let ny = |py: f32| 1.0 - py / h * 2.0;
    let mut v = Vec::new();
    let mut quad = |px: f32, py: f32, pw: f32, ph: f32, c: [f32; 3]| {
        let x0 = nx(px);
        let x1 = nx(px + pw);
        let y0 = ny(py);
        let y1 = ny(py + ph);
        push_quad(&mut v, x0, y0, x1 - x0, y1 - y0, c);
    };
    quad(0.0, 0.0, w, h, bg);
    quad(0.0, 0.0, w, 30.0, surface);
    quad(0.0, 30.0, w, 2.0, accent);
    quad(0.0, 32.0, w * 0.22, h - 58.0, surface);
    quad(w * 0.82, 32.0, w * 0.18, h - 58.0, surface);
    quad(0.0, h - 26.0, w, 26.0, surface);
    quad(0.0, h - 120.0, w, 94.0, surface);
    v
}

fn f32_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vbuf: wgpu::Buffer,
    vcap: usize,
    size: (u32, u32),
}

impl Gpu {
    async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| anyhow::anyhow!("no surface: {e:?}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("no GPU adapter found"))?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("no GPU device: {e:?}"))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let size = (size.width.max(1), size.height.max(1));
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0,
            height: size.1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("keplr"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 1,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 65536,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vbuf,
            vcap: 65536,
            size,
        })
    }

    fn upload(&mut self, verts: &[f32]) {
        let bytes = f32_to_bytes(verts);
        if bytes.len() > self.vcap {
            self.vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: bytes.len() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.vcap = bytes.len();
        }
        self.queue.write_buffer(&self.vbuf, 0, &bytes);
    }

    fn frame(&mut self, clear: wgpu::Color, verts: usize) -> anyhow::Result<()> {
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
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            pass.draw(0..verts as u32, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        texture.present();
        Ok(())
    }
}

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    init_err: Option<String>,
    root: PathBuf,
    open: Option<PathBuf>,
    query: String,
    scene: Option<Scene>,
    dirty: bool,
    frames: u32,
    since: Instant,
}

impl App {
    fn rebuild(&mut self) {
        let spec = SceneSpec {
            root: &self.root,
            open_file: self.open.as_deref(),
            query: self.query.as_str(),
            palette_query: None,
            palette_mode: "files",
            search_query: None,
            left_tab: "project",
            right_tab: "symbols",
            bottom_tab: "terminal",
            width: 100,
        };
        self.scene = Some(super::build_scene(&spec));
        self.dirty = true;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window =
            match event_loop.create_window(WindowAttributes::default().with_title("keplr")) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    self.init_err = Some(format!("no window: {e:?}"));
                    event_loop.exit();
                    return;
                }
            };
        match pollster::block_on(Gpu::new(window.clone())) {
            Ok(g) => {
                self.window = Some(window);
                self.gpu = Some(g);
                self.rebuild();
            }
            Err(e) => {
                self.init_err = Some(format!("{e:#}"));
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(gpu), Some(window)) = (self.gpu.as_mut(), self.window.as_ref())
                {
                    let w = size.width.max(1);
                    let h = size.height.max(1);
                    gpu.config.width = w;
                    gpu.config.height = h;
                    gpu.size = (w, h);
                    gpu.surface.configure(&gpu.device, &gpu.config);
                    let _ = window;
                    self.dirty = true;
                }
            }
            WindowEvent::RedrawRequested => {
                if self.scene.is_none() || self.dirty {
                    self.rebuild();
                }
                let mut failed: Option<String> = None;
                if let (Some(gpu), Some(window)) =
                    (self.gpu.as_mut(), self.window.as_ref())
                {
                    if let Some(scene) = &self.scene {
                        let quads = layout_quads(scene, gpu.size.0, gpu.size.1);
                        let n = quads.len() / 6;
                        gpu.upload(&quads);
                        let bg = parse_hex(&Theme::zed_dark().bg);
                        let clear = wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: 1.0,
                        };
                        if let Err(e) = gpu.frame(clear, n) {
                            failed = Some(format!("{e:#}"));
                        }
                    }
                    self.frames += 1;
                    if self.since.elapsed().as_millis() >= 500 {
                        let fps =
                            self.frames as f64 / self.since.elapsed().as_secs_f64();
                        let label = self
                            .open
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| String::from("(no file)"));
                        window.set_title(&format!(
                            "keplr — {label} — {} — {fps:.0}fps",
                            branch_for(&self.root)
                        ));
                        self.frames = 0;
                        self.since = Instant::now();
                    }
                    self.dirty = false;
                    window.request_redraw();
                }
                if let Some(e) = failed {
                    eprintln!("keplr: gpu frame failed: {e}");
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Released {
                    return;
                }
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::F5) => {
                        self.rebuild();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

pub fn run_desktop(
    root: PathBuf,
    open: Option<PathBuf>,
    query: String,
) -> anyhow::Result<()> {
    let event_loop =
        EventLoop::new().map_err(|e| anyhow::anyhow!("no event loop: {e:?}"))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        window: None,
        gpu: None,
        init_err: None,
        root,
        open,
        query,
        scene: None,
        dirty: true,
        frames: 0,
        since: Instant::now(),
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!("event loop failed: {e:?}"))?;
    if let Some(e) = app.init_err {
        return Err(anyhow::anyhow!("{e}"));
    }
    Ok(())
}

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct GlyphSpot {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub advance: f32,
    pub bx: f32,
    pub by: f32,
}

pub struct GlyphAtlas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub glyphs: HashMap<(char, u32), GlyphSpot>,
}

pub fn build_atlas(font_bytes: &[u8], px: f32) -> anyhow::Result<GlyphAtlas> {
    let font =
        FontRef::try_from_slice(font_bytes).map_err(|e| anyhow::anyhow!("bad font: {e:?}"))?;
    let scaled = font.as_scaled(PxScale::from(px));
    let ascent = scaled.ascent();
    let mut atlas = GlyphAtlas {
        width: 512,
        height: 512,
        pixels: vec![0u8; 512 * 512],
        glyphs: HashMap::new(),
    };
    let mut pen_x = 0u32;
    let mut pen_y = 0u32;
    let mut row_h = 0u32;
    let id = px as u32;
    for b in 32u8..127u8 {
        let c = b as char;
        let glyph = scaled.scaled_glyph(c);
        let gid = glyph.id;
        let outlined = match scaled.outline_glyph(glyph) {
            Some(o) => o,
            None => {
                let adv = scaled.h_advance(gid);
                atlas.glyphs.insert(
                    (c, id),
                    GlyphSpot {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0,
                        advance: adv,
                        bx: 0.0,
                        by: 0.0,
                    },
                );
                continue;
            }
        };
        let bounds = outlined.px_bounds();
        let w = bounds.width() as u32;
        let h = bounds.height() as u32;
        if w == 0 || h == 0 {
            continue;
        }
        if pen_x + w > atlas.width {
            pen_x = 0;
            pen_y += row_h;
            row_h = 0;
        }
        if pen_y + h > atlas.height {
            anyhow::bail!("atlas overflow at {px}px");
        }
        outlined.draw(|x, y, v| {
            let gx = pen_x + x;
            let gy = pen_y + y;
            atlas.pixels[(gy * atlas.width + gx) as usize] =
                (v.clamp(0.0, 1.0) * 255.0) as u8;
        });
        let adv = scaled.h_advance(outlined.glyph().id);
        atlas.glyphs.insert(
            (c, id),
            GlyphSpot {
                x: pen_x,
                y: pen_y,
                w,
                h,
                advance: adv,
                bx: bounds.min.x,
                by: ascent - bounds.min.y,
            },
        );
        pen_x += w + 1;
        row_h = row_h.max(h + 1);
    }
    Ok(atlas)
}

pub fn upload_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &GlyphAtlas,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("keplr-glyphs"),
        size: wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &atlas.pixels,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width),
            rows_per_image: Some(atlas.height),
        },
        wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
    );
    texture
}
