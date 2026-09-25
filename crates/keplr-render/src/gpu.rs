use super::{branch_for, Scene, SceneSpec, Theme};
use std::path::{Path, PathBuf};
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

#[derive(Clone, Copy)]
struct LayoutRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn layout_split_value(node: &serde_json::Value) -> Option<(String, &serde_json::Value)> {
    node.get("Split")
        .and_then(|v| Some((v.get("id")?.as_str()?.to_string(), v)))
}

fn layout_leaf_value(node: &serde_json::Value) -> Option<&serde_json::Value> {
    node.get("Leaf").or_else(|| {
        if node.get("type").and_then(|v| v.as_str()) == Some("leaf") {
            Some(node)
        } else {
            None
        }
    })
}

fn collect_layout_rects(node: &serde_json::Value, rect: LayoutRect, out: &mut Vec<(LayoutRect, bool)>) -> bool {
    if let Some(leaf) = layout_leaf_value(node) {
        let visible = leaf.get("visible").and_then(|v| v.as_bool()).unwrap_or(true);
        out.push((rect, visible));
        return true;
    }
    let split = if let Some((_, value)) = layout_split_value(node) {
        value
    } else if node.get("type").and_then(|v| v.as_str()) == Some("split") {
        node
    } else {
        return false;
    };
    let first = split.get("first");
    let second = split.get("second");
    let ratio = split
        .get("ratio")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.05, 0.95) as f32;
    let axis = split.get("axis").and_then(|v| v.as_str()).unwrap_or("horizontal");
    if axis == "vertical" {
        let first_h = rect.h * ratio;
        if let Some(first) = first {
            collect_layout_rects(first, LayoutRect { h: first_h, ..rect }, out);
        }
        if let Some(second) = second {
            collect_layout_rects(
                second,
                LayoutRect { y: rect.y + first_h, h: rect.h - first_h, ..rect },
                out,
            );
        }
    } else {
        let first_w = rect.w * ratio;
        if let Some(first) = first {
            collect_layout_rects(first, LayoutRect { w: first_w, ..rect }, out);
        }
        if let Some(second) = second {
            collect_layout_rects(
                second,
                LayoutRect { x: rect.x + first_w, w: rect.w - first_w, ..rect },
                out,
            );
        }
    }
    true
}

fn layout_quads(scene: &Scene, w: u32, h: u32) -> Vec<f32> {
    let theme = Theme::amoled();
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
    let mut dynamic = false;
    if let Some(root) = scene
        .layout
        .as_ref()
        .and_then(|value| value.get("tree"))
        .and_then(|tree| tree.get("root"))
    {
        let mut rects = Vec::new();
        dynamic = collect_layout_rects(
            root,
            LayoutRect { x: 0.0, y: 32.0, w, h: h - 58.0 },
            &mut rects,
        );
        if dynamic {
            for (rect, visible) in rects {
                if visible {
                    quad(rect.x, rect.y, rect.w, rect.h, surface);
                    quad(rect.x, rect.y, rect.w, 1.0, bg);
                }
            }
        }
    }
    if !dynamic {
        quad(0.0, 32.0, w * 0.22, h - 58.0, surface);
        quad(w * 0.82, 32.0, w * 0.18, h - 58.0, surface);
        quad(0.0, h - 120.0, w, 94.0, surface);
    }
    quad(0.0, h - 26.0, w, 26.0, surface);
    v
}

fn f32_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

fn create_layout_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("keplr"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
    })
}

pub fn snapshot_scene_png(scene_json: &str, width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    use image::{ImageBuffer, Rgba};
    let scene: Scene =
        serde_json::from_str(scene_json).map_err(|e| anyhow::anyhow!("bad scene: {e}"))?;
    let (w, h) = (width.clamp(64, 4096), height.clamp(64, 4096));
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: true,
        compatible_surface: None,
    }))
    .ok_or_else(|| anyhow::anyhow!("no GPU adapter (not even fallback)"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: None,
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))
    .map_err(|e| anyhow::anyhow!("no GPU device: {e:?}"))?;
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let pipeline = create_layout_pipeline(&device, format);
    let quads = layout_quads(&scene, w, h);
    let vbytes = f32_to_bytes(&quads);
    let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: vbytes.len().max(16) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vbuf, 0, &vbytes);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let theme = Theme::amoled();
    let bg = parse_hex(&theme.bg);
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: bg[0] as f64,
                        g: bg[1] as f64,
                        b: bg[2] as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.draw(0..quads.len() as u32 / 6, 0..1);
    }
    let pitch = (w * 4 + 255) / 256 * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (pitch * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buf,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |v| {
        let _ = tx.send(v);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|_| anyhow::anyhow!("map cancelled"))?
        .map_err(|e| anyhow::anyhow!("map failed: {e:?}"))?;
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    {
        let data = slice.get_mapped_range();
        for y in 0..h as usize {
            let off = y * pitch as usize;
            px.extend_from_slice(&data[off..off + (w * 4) as usize]);
        }
    }
    buf.unmap();
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(w, h, px).ok_or_else(|| anyhow::anyhow!("bad pixels"))?;
    let mut png = Vec::new();
    {
        use image::ImageEncoder;
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
            .map_err(|e| anyhow::anyhow!("png encode: {e}"))?;
    }
    Ok(png)
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
    text_pipeline: wgpu::RenderPipeline,
    text_bind_group: wgpu::BindGroup,
    text_vbuf: wgpu::Buffer,
    text_vcap: usize,
    text_on: bool,
    text_atlas: Option<GlyphAtlas>,
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
        let pipeline = create_layout_pipeline(&device, format);
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 65536,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (text_pipeline, text_bind_group, text_vbuf, text_on, text_atlas) =
            text_stack(&device, &queue, format);
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vbuf,
            vcap: 65536,
            size,
            text_pipeline,
            text_bind_group,
            text_vbuf,
            text_vcap: 65536,
            text_on,
            text_atlas,
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
    text_cache: std::collections::HashMap<u64, (Vec<f32>, Option<[f32; 4]>)>,
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
                        let mut quads = layout_quads(scene, gpu.size.0, gpu.size.1);
                        let key = text_cache_key(scene, gpu.size.0, gpu.size.1);
                        if self.text_cache.len() > 32 {
                            self.text_cache.clear();
                        }
                        let (text_verts, cursor_rect) = match self.text_cache.get(&key) {
                            Some((v, r)) => (v.clone(), *r),
                            None => {
                                let shaped = match &gpu.text_atlas {
                                    Some(atlas) => {
                                        scene_text_quads(atlas, scene, gpu.size.0, gpu.size.1)
                                    }
                                    None => (Vec::new(), None),
                                };
                                self.text_cache.insert(key, shaped.clone());
                                shaped
                            }
                        };
                        if let Some([x, y, w, h]) = cursor_rect {
                            let accent = parse_hex(&Theme::amoled().accent);
                            quads.extend(cursor_px_to_ndc(
                                x,
                                y,
                                w,
                                h,
                                gpu.size.0,
                                gpu.size.1,
                                accent,
                            ));
                        }
                        let n = quads.len() / 6;
                        gpu.upload(&quads);
                        let bg = parse_hex(&Theme::amoled().bg);
                        let clear = wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: 1.0,
                        };
                        if let Err(e) = gpu.frame(clear, n, &text_verts) {
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
        text_cache: std::collections::HashMap::new(),
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

fn token_rgb(kind: keplr_lang::TokenKind) -> [f32; 3] {
    match kind {
        keplr_lang::TokenKind::Keyword => [0.99, 0.37, 0.64],
        keplr_lang::TokenKind::Str => [0.99, 0.42, 0.36],
        keplr_lang::TokenKind::Comment => [0.42, 0.47, 0.53],
        keplr_lang::TokenKind::Number => [0.82, 0.75, 0.41],
        keplr_lang::TokenKind::Type => [0.36, 0.85, 1.0],
        keplr_lang::TokenKind::Function => [0.40, 0.72, 0.64],
        keplr_lang::TokenKind::Macro => [0.99, 0.56, 0.25],
        keplr_lang::TokenKind::Attribute => [0.99, 0.56, 0.25],
        keplr_lang::TokenKind::Constant => [0.63, 0.40, 0.90],
        keplr_lang::TokenKind::Parameter => [0.96, 0.96, 0.97],
        keplr_lang::TokenKind::Punctuation => [0.55, 0.55, 0.58],
        keplr_lang::TokenKind::Other => [0.96, 0.96, 0.97],
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
    let fallback = [keplr_lang::Span {
        start: 0,
        len: line.len(),
        kind: keplr_lang::TokenKind::Other,
    }];
    let spans = if spans.is_empty() { &fallback[..] } else { spans };
    let mut pen = x_px;
    for s in spans {
        let col = token_rgb(s.kind);
        let piece = byte_slice_span(line, s.start, s.len);
        for c in piece.chars() {
            let spot = match atlas.glyphs.get(&(c, 16u32)) {
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

pub fn scene_text_quads(
    atlas: &GlyphAtlas,
    scene: &Scene,
    w_px: u32,
    h_px: u32,
) -> (Vec<f32>, Option<[f32; 4]>) {
    let lang = lang_of(&scene.center.lang);
    let mut v = Vec::new();
    let gutter = 52.0f32;
    let x0 = w_px as f32 * 0.22 + 12.0 + gutter;
    let wrap_x = w_px as f32 * 0.82 - 12.0;
    let mut y = 44.0f32;
    let mut cursor_rect = None;
    for (i, line) in scene.center.lines.iter().take(25).enumerate() {
        let n = scene.center.viewport_top + i;
        let num = format!("{n:>4} ");
        v.extend(layout_colored_line(
            atlas,
            &num,
            &[],
            x0 - gutter,
            y,
            w_px as f32,
            h_px as f32,
        ));
        let spans = keplr_lang::highlight(lang, line);
        let mut row = String::new();
        let mut row_w = 0.0f32;
        let mut rows: Vec<String> = Vec::new();
        for c in line.chars() {
            let adv = atlas
                .glyphs
                .get(&(c, 16u32))
                .map(|g| g.advance)
                .unwrap_or(8.0);
            if row_w + adv > (wrap_x - x0).max(40.0) && !row.is_empty() {
                rows.push(std::mem::take(&mut row));
                row_w = 0.0;
            }
            row.push(c);
            row_w += adv;
        }
        rows.push(row);
        for (ri, sub) in rows.iter().enumerate() {
            // NOTE: spans are line-relative; wrapped continuation rows render plain.
            // Full per-row re-highlighting is the documented next refinement.
            let spans = if ri == 0 { spans.clone() } else { Vec::new() };
            v.extend(layout_colored_line(
                atlas,
                sub,
                &spans,
                x0,
                y,
                w_px as f32,
                h_px as f32,
            ));
            if n == scene.center.cursor.0 && ri == 0 {
                cursor_rect = Some([x0, y, 9.0, 17.0]);
            }
            y += 18.0;
        }
    }
    (v, cursor_rect)
}

fn cursor_px_to_ndc(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    vw: u32,
    vh: u32,
    c: [f32; 3],
) -> Vec<f32> {
    let mut v = Vec::new();
    let nx = |px: f32| px / vw as f32 * 2.0 - 1.0;
    let ny = |py: f32| 1.0 - py / vh as f32 * 2.0;
    push_quad(&mut v, nx(x), ny(y), nx(x + w) - nx(x), ny(y + h) - ny(y), c);
    v
}

fn text_cache_key(scene: &Scene, w: u32, h: u32) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut s = DefaultHasher::new();
    scene.center.viewport_top.hash(&mut s);
    scene.center.cursor.hash(&mut s);
    w.hash(&mut s);
    h.hash(&mut s);
    for line in scene.center.lines.iter().take(25) {
        line.hash(&mut s);
    }
    s.finish()
}

#[allow(clippy::type_complexity)]
fn text_stack(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> (
    wgpu::RenderPipeline,
    wgpu::BindGroup,
    wgpu::Buffer,
    bool,
    Option<GlyphAtlas>,
) {
    let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[],
    });
    let empty_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &empty_layout,
        entries: &[],
    });
    let off_pipeline = |layout: &wgpu::PipelineLayout| {
        let tshader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("keplr-text-off"),
            source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &tshader,
                entry_point: "vs",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    };
    let small_buf = || {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let atlas = match super::discover_font()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| build_atlas(&bytes, 16.0).ok())
    {
        Some(a) => a,
        None => {
            let off_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[],
                    push_constant_ranges: &[],
                });
            return (
                off_pipeline(&off_layout),
                empty_bg,
                small_buf(),
                false,
                None,
            );
        }
    };
    let texture = upload_atlas(device, queue, &atlas);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
    let tshader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("keplr-text"),
        source: wgpu::ShaderSource::Wgsl(TEXT_SHADER.into()),
    });
    let tlayout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let tpipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(&tlayout),
        vertex: wgpu::VertexState {
            module: &tshader,
            entry_point: "vs",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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
            compilation_options: wgpu::PipelineCompilationOptions::default(),
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
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (tpipeline, bg, tvbuf, true, Some(atlas))
}
