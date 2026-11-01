use wasm_bindgen::JsValue;
use web_sys::gpu_shader_stage::{FRAGMENT, VERTEX};
use web_sys::{
    GpuBindGroup, GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBindGroupLayout,
    GpuBindGroupLayoutDescriptor, GpuBindGroupLayoutEntry, GpuBuffer, GpuBufferBinding, GpuDevice,
    GpuSampler, GpuTextureView,
};

pub fn uniform_bgl(device: &GpuDevice) -> Result<GpuBindGroupLayout, JsValue> {
    let entry = GpuBindGroupLayoutEntry::new(0, VERTEX);
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
