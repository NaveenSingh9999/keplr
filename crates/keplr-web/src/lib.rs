use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

fn canvas_by_id(id: &str) -> Result<HtmlCanvasElement, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let el = document
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str("no canvas"))?;
    el.dyn_into::<HtmlCanvasElement>()
        .map_err(|_| JsValue::from_str("not a canvas"))
}

fn ctx2d(canvas: &HtmlCanvasElement) -> Result<CanvasRenderingContext2d, JsValue> {
    let obj = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?;
    let val: JsValue = obj.into();
    val.dyn_into()
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[wasm_bindgen]
pub fn highlight_line(lang: &str, line: &str) -> String {
    let kind =
        keplr_lang::LangKind::from_path(std::path::Path::new(&format!("x.{lang}")));
    let spans = keplr_lang::highlight(kind, line);
    serde_json::to_string(&spans).unwrap_or_else(|_| String::from("[]"))
}

#[wasm_bindgen]
pub fn render_scene(canvas_id: &str, scene_json: &str) -> Result<(), JsValue> {
    let canvas = canvas_by_id(canvas_id)?;
    let ctx = ctx2d(&canvas)?;
    let scene: keplr_render::Scene =
        serde_json::from_str(scene_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    paint(&ctx, &canvas, &scene)
}

fn paint(
    ctx: &CanvasRenderingContext2d,
    canvas: &HtmlCanvasElement,
    scene: &keplr_render::Scene,
) -> Result<(), JsValue> {
    let w = canvas.width() as f64;
    let h = canvas.height() as f64;
    let theme = keplr_render::Theme::zed_dark();
    ctx.set_fill_style(&JsValue::from_str("#0e1116"));
    ctx.fill_rect(0.0, 0.0, w, h);
    ctx.set_font("13px 'JetBrains Mono','SF Mono',monospace");
    ctx.set_fill_style(&JsValue::from_str(&theme.surface));
    ctx.fill_rect(0.0, 0.0, w, 30.0);
    ctx.set_fill_style(&JsValue::from_str(&theme.text));
    ctx.fill_text(&scene.titlebar.root, 12.0, 20.0)?;
    let lw = w * 0.22;
    ctx.set_fill_style(&JsValue::from_str(&theme.surface));
    ctx.fill_rect(0.0, 30.0, lw, h - 56.0);
    ctx.set_fill_style(&JsValue::from_str(&theme.text_dim));
    let mut y = 48.0;
    for line in scene.left.lines.iter().take(30) {
        ctx.fill_text(&line.chars().take(32).collect::<String>(), 12.0, y)?;
        y += 17.0;
    }
    let x0 = lw + 12.0;
    let mut y = 48.0;
    for (i, line) in scene.center.lines.iter().take(40).enumerate() {
        let n = scene.center.viewport_top + i;
        ctx.set_fill_style(&JsValue::from_str(&theme.text_dim));
        ctx.fill_text(&format!("{n:>3}"), x0, y)?;
        ctx.set_fill_style(&JsValue::from_str(&theme.text));
        ctx.fill_text(&line.chars().take(100).collect::<String>(), x0 + 44.0, y)?;
        y += 17.0;
    }
    let cy = 48.0
        + ((scene
            .center
            .cursor
            .0
            .saturating_sub(scene.center.viewport_top)) as f64)
            * 17.0;
    ctx.set_fill_style(&JsValue::from_str(&theme.accent));
    ctx.fill_rect(x0 + 44.0, cy - 12.0, 8.0, 15.0);
    ctx.set_fill_style(&JsValue::from_str(&theme.surface));
    ctx.fill_rect(0.0, h - 26.0, w, 26.0);
    ctx.set_fill_style(&JsValue::from_str(&theme.text_dim));
    ctx.fill_text(
        &format!("{} · {} files", scene.status.branch, scene.status.files),
        12.0,
        h - 9.0,
    )?;
    if scene.palette_open {
        ctx.set_fill_style(&JsValue::from_str("#161b22ee"));
        ctx.fill_rect(w * 0.25, 60.0, w * 0.5, 220.0);
        ctx.set_fill_style(&JsValue::from_str(&theme.text));
        ctx.fill_text(&scene.palette_query, w * 0.25 + 12.0, 84.0)?;
        let mut y = 106.0;
        for hit in scene.palette_hits.iter().take(8) {
            ctx.fill_text(hit, w * 0.25 + 12.0, y)?;
            y += 17.0;
        }
    }
    Ok(())
}
