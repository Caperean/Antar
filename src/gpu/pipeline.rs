use web_sys::{
    GpuBindGroupLayout, GpuDevice,
    GpuFragmentState, GpuPipelineLayout, GpuPipelineLayoutDescriptor, GpuRenderPipeline,
    GpuRenderPipelineDescriptor, GpuVertexAttribute, GpuVertexBufferLayout, GpuVertexState,
};
use wasm_bindgen::JsValue;
use js_sys::JsOption;

use super::surface::SURFACE_FORMAT;

const COMMON_WGSL: &str = include_str!("shaders/common.wgsl");
const SPRITE_BODY: &str = include_str!("shaders/sprite.wgsl");
const RECT_BODY: &str = include_str!("shaders/rect.wgsl");
const RASTERIZER_BODY: &str = include_str!("shaders/rasterizer.wgsl");

fn sprite_wgsl() -> String {
    format!("{COMMON_WGSL}\n{SPRITE_BODY}")
}

fn rect_wgsl() -> String {
    format!("{COMMON_WGSL}\n{RECT_BODY}")
}

fn rasterizer_wgsl() -> String {
    RASTERIZER_BODY.to_string()
}

pub struct Pipelines {
    pub sprite: GpuRenderPipeline,
    pub rect: GpuRenderPipeline,
    // Compute pipeline requires web-sys with GpuComputePipeline support
    // When available, uncomment build_rasterizer_pipeline below
}
 
impl Pipelines {
    pub fn new(
        device: &GpuDevice,
        uniform_bgl: &GpuBindGroupLayout,
        texture_bgl: &GpuBindGroupLayout,
    ) -> Result<Self, JsValue> {
        Ok(Self {
            sprite: build_sprite_pipeline(device, uniform_bgl, texture_bgl)?,
            rect:   build_rect_pipeline(device, uniform_bgl)?,
        })
    }
}
 
fn build_sprite_pipeline(
    device: &GpuDevice,
    uniform_bgl: &GpuBindGroupLayout,
    texture_bgl: &GpuBindGroupLayout,
) -> Result<GpuRenderPipeline, JsValue> {
    let source = sprite_wgsl();
    let shader = device.create_shader_module(&web_sys::GpuShaderModuleDescriptor::new(&source));
 
    let layout_desc = GpuPipelineLayoutDescriptor::new(&[
        JsOption::from_option(Some(uniform_bgl.clone())),
        JsOption::from_option(Some(texture_bgl.clone())),
    ]);
    let layout: GpuPipelineLayout = device.create_pipeline_layout(&layout_desc);
 
    // Buffer 0: per-vertex  — pos (loc 0, offset 0) + uv (loc 1, offset 8), stride 16
    let a0 = GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32x2, 0, 0);
    let a1 = GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32x2, 8, 1);
    let mut vb0 = GpuVertexBufferLayout::new(16, &[a0, a1]);
 
    // Buffer 1: per-instance — same 72-byte / 18-float layout as before, but
    // packed into vec4/vec2 attributes instead of 18 individual Float32
    // locations. This matters: 2 (vb0: pos+uv) + 18 (vb1, one-per-float) = 20
    // vertex attributes, which exceeds WebGPU's default `maxVertexAttributes`
    // limit of 16 (device.rs requests no elevated limits). That overflow was
    // present even before the tint fix (2 + 15 = 17), and silently failed
    // pipeline creation — WebGPU validation errors from `createRenderPipeline`
    // surface asynchronously via the error-scope / `uncapturederror` path, not
    // as a catchable JS exception, so `Antar::init()` / `present()` never threw
    // and sprites just silently never rendered.
    //
    // loc 2 = i_dst   (vec4: x, y, w, h)
    // loc 3 = i_src   (vec4: x, y, w, h)
    // loc 4 = i_color (vec4: r, g, b, a)
    // loc 5 = i_transform_abcd (vec4: a, b, c, d)
    // loc 6 = i_transform_t    (vec2: tx, ty)
    let instance_attrs = [
        GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32x4, 0, 2),
        GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32x4, 16, 3),
        GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32x4, 32, 4),
        GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32x4, 48, 5),
        GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32x2, 64, 6),
    ];
    let mut vb1 = GpuVertexBufferLayout::new(72, &instance_attrs);
    vb1.set_step_mode(web_sys::GpuVertexStepMode::Instance);
 
    let mut vs = GpuVertexState::new(&shader);
    vs.set_entry_point("vs_main");
    vs.set_buffers(&[
        JsOption::from_option(Some(vb0)),
        JsOption::from_option(Some(vb1)),
    ]);
 
    let mut fs = GpuFragmentState::new(&shader, &make_color_targets()?);
    fs.set_entry_point("fs_main");
 
    let mut desc = GpuRenderPipelineDescriptor::new(&layout, &vs);
    desc.set_fragment(&fs);
    desc.set_primitive(&web_sys::GpuPrimitiveState::new());
 
    device.create_render_pipeline(&desc)
}
 
fn build_rect_pipeline(
    device: &GpuDevice,
    uniform_bgl: &GpuBindGroupLayout,
) -> Result<GpuRenderPipeline, JsValue> {
    let source = rect_wgsl();
    let shader = device.create_shader_module(&web_sys::GpuShaderModuleDescriptor::new(&source));
 
    let layout_desc =
        GpuPipelineLayoutDescriptor::new(&[JsOption::from_option(Some(uniform_bgl.clone()))]);
    let layout: GpuPipelineLayout = device.create_pipeline_layout(&layout_desc);
 
    // Buffer 0: per-vertex — shared quad buffer is stride 16 (pos + uv); rect uses only pos.
    let a0 = GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32x2, 0, 0);
    let mut vb0 = GpuVertexBufferLayout::new(16, &[a0]);
 
    // Buffer 1: per-instance — 14 × f32 (loc 1..14), stride 56
    // loc: 1=i_rect_x 2=i_rect_y 3=i_rect_w 4=i_rect_h
    //      5=i_color_r 6=i_color_g 7=i_color_b 8=i_color_a
    //      9=i_ta 10=i_tb 11=i_tc 12=i_td 13=i_tx 14=i_ty
    let instance_attrs: Vec<GpuVertexAttribute> = (0u32..14)
        .map(|i| GpuVertexAttribute::new(web_sys::GpuVertexFormat::Float32, i * 4, i + 1))
        .collect();
    let mut vb1 = GpuVertexBufferLayout::new(56, &instance_attrs);
    vb1.set_step_mode(web_sys::GpuVertexStepMode::Instance);
 
    let mut vs = GpuVertexState::new(&shader);
    vs.set_entry_point("vs_main");
    vs.set_buffers(&[
        JsOption::from_option(Some(vb0)),
        JsOption::from_option(Some(vb1)),
    ]);
 
    let mut fs = GpuFragmentState::new(&shader, &make_color_targets()?);
    fs.set_entry_point("fs_main");
 
    let mut desc = GpuRenderPipelineDescriptor::new(&layout, &vs);
    desc.set_fragment(&fs);
    desc.set_primitive(&web_sys::GpuPrimitiveState::new());
 
    device.create_render_pipeline(&desc)
}
 
fn make_color_targets() -> Result<[JsOption<web_sys::GpuColorTargetState>; 1], JsValue> {
    let mut target = web_sys::GpuColorTargetState::new(SURFACE_FORMAT);
 
    let blend = web_sys::GpuBlendState::new(
        &premul_alpha_blend_component(),
        &premul_alpha_blend_component(),
    );
    target.set_blend(&blend);
 
    Ok([JsOption::from_option(Some(target))])
}
 
fn premul_alpha_blend_component() -> web_sys::GpuBlendComponent {
    let mut c = web_sys::GpuBlendComponent::new();
    c.set_src_factor(web_sys::GpuBlendFactor::One);
    c.set_dst_factor(web_sys::GpuBlendFactor::OneMinusSrcAlpha);
    c.set_operation(web_sys::GpuBlendOperation::Add);
    c
}

// Compute pipeline for polygon rasterization (rasterizer.wgsl)
// Requires web-sys version with GpuComputePipeline support
// When available, this will be uncommented and integrated:
// fn build_rasterizer_pipeline(
//     device: &GpuDevice,
//     uniform_bgl: &GpuBindGroupLayout,
//     texture_bgl: &GpuBindGroupLayout,
// ) -> Result<GpuComputePipeline, JsValue> {
//     let source = rasterizer_wgsl();
//     let shader = device.create_shader_module(&web_sys::GpuShaderModuleDescriptor::new(&source));
//
//     let layout_desc = GpuPipelineLayoutDescriptor::new(&[
//         JsOption::from_option(Some(uniform_bgl.clone())),
//         JsOption::from_option(Some(texture_bgl.clone())),
//     ]);
//     let layout: GpuPipelineLayout = device.create_pipeline_layout(&layout_desc);
//
//     let mut cs = web_sys::GpuComputeState::new(&shader);
//     cs.set_entry_point("main");
//
//     let mut desc = GpuComputePipelineDescriptor::new(&layout, &cs);
//
//     device.create_compute_pipeline(&desc)
// }