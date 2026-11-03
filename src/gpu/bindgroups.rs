use wasm_bindgen::JsValue;
use web_sys::gpu_shader_stage::{FRAGMENT, VERTEX};
use web_sys::{
    GpuBindGroup, GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBindGroupLayout,
    GpuBindGroupLayoutDescriptor, GpuBindGroupLayoutEntry, GpuBuffer, GpuBufferBinding, GpuDevice,
    GpuSampler, GpuTextureView,
};

pub fn uniform_bgl(device: &GpuDevice) -> Result<GpuBindGroupLayout, JsValue> {
    // VERTEX | FRAGMENT: the sprite/rect vertex shaders only read `proj`, but
    // the polygon fragment shader reads the same uniform too. Wider visibility
    // than a pipeline actually uses is legal; narrower is not.
    let entry = GpuBindGroupLayoutEntry::new(0, VERTEX | FRAGMENT);
    entry.set_buffer(&web_sys::GpuBufferBindingLayout::new());

    let entries = [entry];
    let desc = GpuBindGroupLayoutDescriptor::new(&entries);
    device.create_bind_group_layout(&desc)
}

pub fn texture_bgl(device: &GpuDevice) -> Result<GpuBindGroupLayout, JsValue> {
    let tex_entry = GpuBindGroupLayoutEntry::new(0, FRAGMENT);
    tex_entry.set_texture(&web_sys::GpuTextureBindingLayout::new());

    let samp_entry = GpuBindGroupLayoutEntry::new(1, FRAGMENT);
    samp_entry.set_sampler(&web_sys::GpuSamplerBindingLayout::new());

    let entries = [tex_entry, samp_entry];
    let desc = GpuBindGroupLayoutDescriptor::new(&entries);
    device.create_bind_group_layout(&desc)
}

/// Storage-buffer layout for the polygon shader: metadata at binding 0,
/// pre-transformed vertices at binding 1. Both stages read them — the vertex
/// shader needs the bounding box, the fragment shader the contour.
pub fn polygon_bgl(device: &GpuDevice) -> Result<GpuBindGroupLayout, JsValue> {
    let storage = web_sys::GpuBufferBindingLayout::new();
    // `var<storage, read>` needs read-only-storage, not storage.
    storage.set_type(web_sys::GpuBufferBindingType::ReadOnlyStorage);

    let meta_entry = GpuBindGroupLayoutEntry::new(0, VERTEX | FRAGMENT);
    meta_entry.set_buffer(&storage);

    let vertex_entry = GpuBindGroupLayoutEntry::new(1, VERTEX | FRAGMENT);
    vertex_entry.set_buffer(&storage);

    let entries = [meta_entry, vertex_entry];
    let desc = GpuBindGroupLayoutDescriptor::new(&entries);
    device.create_bind_group_layout(&desc)
}

pub fn create_uniform_bg(
    device: &GpuDevice,
    layout: &GpuBindGroupLayout,
    buffer: &GpuBuffer,
    size: u32,
) -> Result<GpuBindGroup, JsValue> {
    let binding = GpuBufferBinding::new(buffer);
    binding.set_size(size);

    let entry = GpuBindGroupEntry::new_with_gpu_buffer_binding(0, &binding);
    let entries = [entry];
    let desc = GpuBindGroupDescriptor::new(&entries, layout);
    Ok(device.create_bind_group(&desc))
}

pub fn create_texture_bg(
    device: &GpuDevice,
    layout: &GpuBindGroupLayout,
    view: &GpuTextureView,
    sampler: &GpuSampler,
) -> Result<GpuBindGroup, JsValue> {
    let tex_entry = GpuBindGroupEntry::new_with_gpu_texture_view(0, view);
    let samp_entry = GpuBindGroupEntry::new(1, sampler);
    let entries = [tex_entry, samp_entry];
    let desc = GpuBindGroupDescriptor::new(&entries, layout);
    Ok(device.create_bind_group(&desc))
}

/// Bind the whole polygon buffers. Both are exact multiples of the shader's
/// element strides (96 bytes of metadata, 8 bytes per vertex), so the array
/// length the shader sees matches what was uploaded.
pub fn create_polygon_bg(
    device: &GpuDevice,
    layout: &GpuBindGroupLayout,
    meta: &GpuBuffer,
    vertices: &GpuBuffer,
) -> Result<GpuBindGroup, JsValue> {
    let meta_entry =
        GpuBindGroupEntry::new_with_gpu_buffer_binding(0, &GpuBufferBinding::new(meta));
    let vertex_entry =
        GpuBindGroupEntry::new_with_gpu_buffer_binding(1, &GpuBufferBinding::new(vertices));
    let entries = [meta_entry, vertex_entry];
    let desc = GpuBindGroupDescriptor::new(&entries, layout);
    Ok(device.create_bind_group(&desc))
}
