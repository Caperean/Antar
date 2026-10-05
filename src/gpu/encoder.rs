use wasm_bindgen::JsValue;
use web_sys::{
    GpuCanvasContext, GpuColorDict, GpuLoadOp, GpuRenderPassColorAttachment,
    GpuRenderPassDescriptor, GpuRenderPassEncoder, GpuStoreOp, GpuTextureView,
};
use js_sys::JsOption;

pub fn begin_render_pass(
    encoder: &web_sys::GpuCommandEncoder,
    surface_view: &GpuTextureView,
    clear_r: f64,
    clear_g: f64,
    clear_b: f64,
    clear_a: f64,
) -> Result<GpuRenderPassEncoder, JsValue> {
    let mut attachment = GpuRenderPassColorAttachment::new_with_gpu_texture_view(
        GpuLoadOp::Clear,
        GpuStoreOp::Store,
        surface_view,
    );
    let color = GpuColorDict::new(clear_a, clear_b, clear_g, clear_r);
    attachment.set_clear_value_gpu_color_dict(&color);

    let attachments = [JsOption::from_option(Some(attachment))];
    let pass_desc = GpuRenderPassDescriptor::new(&attachments);
    encoder.begin_render_pass(&pass_desc)
}

pub fn current_texture_view(ctx: &GpuCanvasContext) -> Result<GpuTextureView, JsValue> {
    ctx.get_current_texture()?.create_view()
}
