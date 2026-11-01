use js_sys::Float32Array;
use wasm_bindgen::JsValue;
use web_sys::gpu_buffer_usage::{COPY_DST, INDEX, STORAGE, UNIFORM, VERTEX};
use web_sys::{GpuBuffer, GpuBufferDescriptor, GpuDevice};

/// Unit quad: two triangles covering [0,1]x[0,1].
/// Layout: x, y, u, v
#[rustfmt::skip]
pub const QUAD_VERTICES: [f32; 16] = [
    0.0, 0.0,  0.0, 0.0,
    1.0, 0.0,  1.0, 0.0,
    0.0, 1.0,  0.0, 1.0,
    1.0, 1.0,  1.0, 1.0,
];

pub const QUAD_INDICES: [u16; 6] = [0, 1, 2, 2, 1, 3];

pub fn create_vertex_buffer(device: &GpuDevice) -> Result<GpuBuffer, JsValue> {
    let data = Float32Array::from(QUAD_VERTICES.as_ref());
    let size = (QUAD_VERTICES.len() * 4) as f64;
    let desc = GpuBufferDescriptor::new_with_f64(size, VERTEX | COPY_DST);
    let buf = device.create_buffer(&desc)?;
    device
        .queue()
        .write_buffer_with_u32_and_buffer_source(&buf, 0, &data)?;
    Ok(buf)
}

pub fn create_index_buffer(device: &GpuDevice) -> Result<GpuBuffer, JsValue> {
    let data = js_sys::Uint16Array::from(QUAD_INDICES.as_ref());
    let size = (QUAD_INDICES.len() * 2) as f64;
    let desc = GpuBufferDescriptor::new_with_f64(size, INDEX | COPY_DST);
    let buf = device.create_buffer(&desc)?;
    device
        .queue()
        .write_buffer_with_u32_and_buffer_source(&buf, 0, &data)?;
    Ok(buf)
}

/// Create a mappable uniform buffer containing a 4x4 f32 matrix (64 bytes).
pub fn create_uniform_buffer(device: &GpuDevice) -> Result<GpuBuffer, JsValue> {
    let desc = GpuBufferDescriptor::new_with_f64(64.0, UNIFORM | COPY_DST);
    device.create_buffer(&desc)
}

/// Create a dynamic instance buffer large enough for `capacity` instances.
/// Each instance is `stride` bytes (caller defines the layout).
pub fn create_instance_buffer(
    device: &GpuDevice,
    capacity: usize,
    stride: usize,
) -> Result<GpuBuffer, JsValue> {
    let size = (capacity * stride) as f64;
    let desc = GpuBufferDescriptor::new_with_f64(size, VERTEX | COPY_DST);
    device.create_buffer(&desc)
}

/// Create a storage buffer for compute shader read/write operations.
pub fn create_storage_buffer(
    device: &GpuDevice,
    capacity: usize,
    stride: usize,
) -> Result<GpuBuffer, JsValue> {
    let size = (capacity * stride) as f64;
    let desc = GpuBufferDescriptor::new_with_f64(size, STORAGE | COPY_DST);
    device.create_buffer(&desc)
}

/// Build an orthographic projection matrix that maps pixel coords to NDC.
/// Returns a flat [f32; 16] in column-major order for a mat4x4.
pub fn ortho_matrix(width: u32, height: u32) -> [f32; 16] {
    let w = width as f32;
    let h = height as f32;
    [
        2.0 / w,
        0.0,
        0.0,
        0.0,
        0.0,
        -2.0 / h,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        -1.0,
        1.0,
        0.0,
        1.0,
    ]
}
