//surface.rs
use wasm_bindgen::prelude::*;
use web_sys::{
    GpuCanvasConfiguration, GpuCanvasContext, GpuDevice, GpuTextureFormat, HtmlCanvasElement,
};

pub const SURFACE_FORMAT: GpuTextureFormat = GpuTextureFormat::Bgra8unorm;

fn build_config(device: &GpuDevice) -> GpuCanvasConfiguration {
    // No explicit `size`: the texture tracks the canvas dimensions, so a resize
    // does not by itself invalidate the configuration.
    let cfg = GpuCanvasConfiguration::new(device, SURFACE_FORMAT);
    cfg.set_alpha_mode(web_sys::GpuCanvasAlphaMode::Premultiplied);
    cfg
}

pub fn configure_surface(
    canvas: &HtmlCanvasElement,
    device: &GpuDevice,
) -> Result<GpuCanvasContext, JsValue> {
    let ctx: GpuCanvasContext = canvas
        .get_context("webgpu")?
        .ok_or_else(|| JsValue::from_str("failed to get WebGPU context"))?
        .dyn_into()?;

    ctx.configure(&build_config(device))?;

    Ok(ctx)
}

/// Re-apply the configuration after the canvas changed size.
///
/// The projection matrix is derived from the stored dimensions, so a stale size
/// stretches every draw even though the surface itself would follow the canvas.
/// Reconfiguring also covers implementations that snapshot the size when the
/// context was first configured.
pub fn reconfigure(device: &GpuDevice, ctx: &GpuCanvasContext) -> Result<(), JsValue> {
    ctx.configure(&build_config(device))
}
