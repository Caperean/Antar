//surface.rs
use wasm_bindgen::prelude::*;
use web_sys::{
    GpuCanvasConfiguration, GpuCanvasContext, GpuDevice, GpuTextureFormat, HtmlCanvasElement,
};

pub const SURFACE_FORMAT: GpuTextureFormat = GpuTextureFormat::Bgra8unorm;

pub fn configure_surface(
    canvas: &HtmlCanvasElement,
    device: &GpuDevice,
) -> Result<GpuCanvasContext, JsValue> {
    let ctx: GpuCanvasContext = canvas
        .get_context("webgpu")?
        .ok_or_else(|| JsValue::from_str("failed to get WebGPU context"))?
        .dyn_into()?;

    let cfg = GpuCanvasConfiguration::new(device, SURFACE_FORMAT);
    cfg.set_alpha_mode(web_sys::GpuCanvasAlphaMode::Premultiplied);
    ctx.configure(&cfg)?;

    Ok(ctx)
}
