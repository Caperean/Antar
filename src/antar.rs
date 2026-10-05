//! Public WASM API for **Antar** — a 2D renderer with Canvas-style API and WebGPU backend.
//!
//! Thin wrappers translate JS-friendly types into the internal renderer.
//! CPU logic lives in `renderer/`; GPU execution in `gpu/`.

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::gpu::GpuContext;
use crate::renderer::{
    atlas::{create_texture_atlas, AtlasSource},
    command::{DrawCommand, TransformOp},
    params::{ImageDrawParams, PolygonDrawParams, RectDrawParams},
    types::{finite_f32_from_f32, Color, TextureHandle},
    Renderer2D,
};

#[wasm_bindgen]
pub struct Antar {
    renderer: Renderer2D,
    gpu: GpuContext,
}

#[wasm_bindgen]
impl Antar {
    /// Initialise Antar on a canvas element.
    ///
    /// ```js
    /// const ctx = await Antar.init(document.getElementById("canvas"));
    /// ```
    #[wasm_bindgen]
    pub async fn init(canvas: HtmlCanvasElement) -> Result<Antar, JsValue> {
        let gpu = GpuContext::init(&canvas).await?;
        let renderer = Renderer2D::new();
        Ok(Antar { renderer, gpu })
    }

    /// Clear the canvas. `color` is a CSS hex string, e.g. `"#1a1a2e"`.
    #[wasm_bindgen]
    pub fn clear(&mut self, color: &str) {
        let c = Color::from_hex(color).unwrap_or(Color::BLACK);
        self.renderer.push_command(DrawCommand::Clear(c));
    }

    /// Flush all queued draw commands to the GPU and present the frame.
    #[wasm_bindgen]
    pub fn present(&mut self) -> Result<(), JsValue> {
        let commands = self.renderer.flush_commands();
        self.gpu.execute(&commands)
    }

    #[wasm_bindgen(js_name = drawImage)]
    pub fn draw_image(&mut self, js_params: JsValue) -> Result<(), JsValue> {
        let params = ImageDrawParams::from_js(&js_params)?;
        if !self.gpu.has_texture(params.texture) {
            return Err(JsValue::from_str(&format!(
                "drawImage: unknown texture handle {} (was it loaded, or already unloaded?)",
                params.texture.0
            )));
        }
        self.renderer.push_command(DrawCommand::DrawImage(params));
        Ok(())
    }

    #[wasm_bindgen(js_name = fillRect)]
    pub fn fill_rect(&mut self, js_params: JsValue) -> Result<(), JsValue> {
        let params = RectDrawParams::from_js(&js_params)?;
        self.renderer.push_command(DrawCommand::FillRect(params));
        Ok(())
    }

    #[wasm_bindgen(js_name = fillPolygon)]
    pub fn fill_polygon(&mut self, js_params: JsValue) -> Result<(), JsValue> {
        let params = PolygonDrawParams::from_js(&js_params)?;
        if let Some(texture_id) = params.texture {
            if !self.gpu.has_texture(texture_id) {
                return Err(JsValue::from_str(&format!(
                    "fillPolygon: unknown texture handle {} (was it loaded, or already unloaded?)",
                    texture_id.0
                )));
            }
        }
        self.renderer.push_command(DrawCommand::FillPolygon(params));
        Ok(())
    }

    #[wasm_bindgen(js_name = setFillStyle)]
    pub fn set_fill_style(&mut self, css: &str) {
        let color = Color::from_css(css).unwrap_or(Color::WHITE);
        self.renderer.push_command(DrawCommand::SetFillStyle(color));
    }

    #[wasm_bindgen(js_name = setGlobalAlpha)]
    pub fn set_global_alpha(&mut self, alpha: f32) -> Result<(), JsValue> {
        let alpha = finite_f32_from_f32(alpha, "alpha")?;
        self.renderer.push_command(DrawCommand::SetAlpha(alpha));
        Ok(())
    }

    #[wasm_bindgen]
    pub fn translate(&mut self, x: f32, y: f32) -> Result<(), JsValue> {
        let x = finite_f32_from_f32(x, "x")?;
        let y = finite_f32_from_f32(y, "y")?;
        self.renderer
            .push_command(DrawCommand::Transform(TransformOp::Translate(x, y)));
        Ok(())
    }

    #[wasm_bindgen]
    pub fn rotate(&mut self, angle: f32) -> Result<(), JsValue> {
        let angle = finite_f32_from_f32(angle, "angle")?;
        self.renderer
            .push_command(DrawCommand::Transform(TransformOp::Rotate(angle)));
        Ok(())
    }

    #[wasm_bindgen]
    pub fn scale(&mut self, sx: f32, sy: f32) -> Result<(), JsValue> {
        let sx = finite_f32_from_f32(sx, "sx")?;
        let sy = finite_f32_from_f32(sy, "sy")?;
        self.renderer
            .push_command(DrawCommand::Transform(TransformOp::Scale(sx, sy)));
        Ok(())
    }

    #[wasm_bindgen]
    pub fn save(&mut self) {
        self.renderer.push_command(DrawCommand::PushState);
    }

    #[wasm_bindgen]
    pub fn restore(&mut self) {
        self.renderer.push_command(DrawCommand::PopState);
    }

    /// Upload RGBA8 pixel data as a GPU texture. Returns a texture handle (u32).
    #[wasm_bindgen(js_name = loadTexture)]
    pub fn load_texture(
        &mut self,
        rgba_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<u32, JsValue> {
        let handle = self.gpu.load_texture(rgba_data, width, height)?;
        Ok(handle.0)
    }

    #[wasm_bindgen(js_name = unloadTexture)]
    pub fn unload_texture(&mut self, texture_id: u32) {
        self.gpu.unload_texture(TextureHandle(texture_id));
    }

    #[wasm_bindgen(js_name = createTextureAtlas)]
    pub fn create_texture_atlas_js(
        &mut self,
        keys: Vec<String>,
        widths: Vec<u32>,
        heights: Vec<u32>,
        data_flat: &[u8],
        atlas_size: u32,
    ) -> Result<JsValue, JsValue> {
        if keys.len() != widths.len() || keys.len() != heights.len() {
            return Err(JsValue::from_str(
                "keys, widths, heights must have equal length",
            ));
        }

        let mut offset = 0usize;
        let mut sources = Vec::with_capacity(keys.len());
        for i in 0..keys.len() {
            let pixel_count = (widths[i] as u64)
                .checked_mul(heights[i] as u64)
                .and_then(|n| n.checked_mul(4))
                .ok_or_else(|| JsValue::from_str("sprite dimensions overflow"))?;
            let len = pixel_count as usize;
            let end = offset
                .checked_add(len)
                .ok_or_else(|| JsValue::from_str("data_flat offset overflow"))?;
            if end > data_flat.len() {
                return Err(JsValue::from_str(&format!(
                    "data_flat too short: need {end} bytes, got {}",
                    data_flat.len()
                )));
            }
            sources.push(AtlasSource {
                key: keys[i].clone(),
                width: widths[i],
                height: heights[i],
                data: data_flat[offset..end].to_vec(),
            });
            offset = end;
        }

        if offset != data_flat.len() {
            return Err(JsValue::from_str(&format!(
                "data_flat has {extra} trailing bytes",
                extra = data_flat.len() - offset
            )));
        }

        let mut atlas = create_texture_atlas(sources, atlas_size)
            .ok_or_else(|| JsValue::from_str("atlas packing failed: atlas too small"))?;

        let handle = self.gpu.load_texture(&atlas.pixels, atlas.width, atlas.height)?;
        let atlas_id = handle.0;
        atlas.set_atlas_id(atlas_id);

        let result = js_sys::Object::new();
        js_sys::Reflect::set(&result, &"atlasId".into(), &JsValue::from(atlas_id))?;

        let uvs_obj = js_sys::Object::new();
        for (key, entry) in &atlas.entries {
            let uv_arr = js_sys::Array::new();
            uv_arr.push(&JsValue::from(entry.uv.x));
            uv_arr.push(&JsValue::from(entry.uv.y));
            uv_arr.push(&JsValue::from(entry.uv.w));
            uv_arr.push(&JsValue::from(entry.uv.h));
            js_sys::Reflect::set(&uvs_obj, &key.as_str().into(), &uv_arr)?;
        }
        js_sys::Reflect::set(&result, &"uvs".into(), &uvs_obj)?;

        Ok(result.into())
    }

    // Global renderer configuration (from zalozenia.txt point 2b)
    // These configure polygon rendering behavior (when compute shader is enabled)
    #[wasm_bindgen(js_name = setDebugMode)]
    pub fn set_debug_mode(&mut self, mode: u32) {
        self.gpu.set_debug_mode(mode);
    }

    #[wasm_bindgen(js_name = setOutputGamma)]
    pub fn set_output_gamma(&mut self, gamma: f32) {
        self.gpu.set_output_gamma(gamma);
    }

    #[wasm_bindgen(js_name = setMinPolygonAreaPx)]
    pub fn set_min_polygon_area_px(&mut self, area: f32) {
        self.gpu.set_min_polygon_area_px(area);
    }

    #[wasm_bindgen(js_name = setEdgeSoftness)]
    pub fn set_edge_softness(&mut self, softness: f32) {
        self.gpu.set_edge_softness(softness);
    }
}
