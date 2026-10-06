pub mod bindgroups;
pub mod buffers;
pub mod device;
pub mod encoder;
pub mod pipeline;
pub mod surface;
pub mod texture;

use wasm_bindgen::prelude::*;
use web_sys::{GpuBindGroup, GpuBuffer, GpuCanvasContext, GpuDevice, GpuQueue, HtmlCanvasElement};
use js_sys::Float32Array;

use crate::renderer::{
    command::FrameBatch,
    types::{Color, TextureHandle},
};
use buffers::{
    create_index_buffer, create_instance_buffer, create_storage_buffer, create_uniform_buffer,
    create_vertex_buffer, ortho_matrix,
};
use bindgroups::{create_uniform_bg, texture_bgl, uniform_bgl};
use pipeline::Pipelines;
use surface::configure_surface;
use texture::TextureRegistry;

const INSTANCE_CAPACITY: usize = 8192; // Increased for better batching
/// Matches sprite pipeline: 18 × f32 attributes (was 15 — grew by 3 to carry
/// the r/g/b tint channels down to the shader; see gpu/shaders/sprite.wgsl).
const SPRITE_FLOATS: usize = 18;
const SPRITE_INSTANCE_STRIDE: usize = SPRITE_FLOATS * 4;
/// Matches rect pipeline: 14 × f32 attributes.
const RECT_FLOATS: usize = 14;
const RECT_INSTANCE_STRIDE: usize = RECT_FLOATS * 4;

/// Polygon buffer capacities (increased for better batching)
const MAX_POLYGONS: usize = 512;
const MAX_POLYGON_VERTICES: usize = 256; // Matches shader MAX_VERTICES
const POLYGON_META_STRIDE: usize = 64; // 16 floats × 4 bytes
const VERTEX_STRIDE: usize = 8; // vec2<f32> = 2 floats × 4 bytes

/// WebGPU backend — owns device, surface, textures and draws instances from `Renderer2D`.
pub struct GpuContext {
    device: GpuDevice,
    queue: GpuQueue,
    surface: GpuCanvasContext,
    pipelines: Pipelines,

    vertex_buf: GpuBuffer,
    index_buf: GpuBuffer,
    uniform_buf: GpuBuffer,
    sprite_instance_buf: GpuBuffer,
    rect_instance_buf: GpuBuffer,

    uniform_bg: GpuBindGroup,
    texture_bgl: web_sys::GpuBindGroupLayout,
    sampler: web_sys::GpuSampler,

    textures: TextureRegistry,
    width: u32,
    height: u32,

    sprite_data: Vec<f32>,
    rect_data: Vec<f32>,
    sprite_tex_ids: Vec<u32>,

    // Polygon rendering infrastructure
    polygon_meta_buf: GpuBuffer,
    polygon_vertex_buf: GpuBuffer,
    polygon_data: Vec<f32>,      // Flattened polygon metadata
    polygon_vertices: Vec<f32>,  // Flattened vertex coordinates

    // Global renderer configuration (from zalozenia.txt point 2b)
    debug_mode: u32,
    output_gamma: f32,
    min_polygon_area_px: f32,
    edge_softness: f32,
}

impl GpuContext {
    pub async fn init(canvas: &HtmlCanvasElement) -> Result<Self, JsValue> {
        let (_adapter, device, queue) = device::init_device().await?;
        let width = canvas.width();
        let height = canvas.height();
        let surface = configure_surface(canvas, &device)?;

        let uniform_bgl = uniform_bgl(&device)?;
        let texture_bgl_layout = texture_bgl(&device)?;
        let pipelines = Pipelines::new(&device, &uniform_bgl, &texture_bgl_layout)?;

        let vertex_buf = create_vertex_buffer(&device)?;
        let index_buf = create_index_buffer(&device)?;
        let uniform_buf = create_uniform_buffer(&device)?;
        let sprite_instance_buf =
            create_instance_buffer(&device, INSTANCE_CAPACITY, SPRITE_INSTANCE_STRIDE)?;
        let rect_instance_buf =
            create_instance_buffer(&device, INSTANCE_CAPACITY, RECT_INSTANCE_STRIDE)?;
        let uniform_bg = create_uniform_bg(&device, &uniform_bgl, &uniform_buf, 64)?;

        let mut sampler_desc = web_sys::GpuSamplerDescriptor::new();
        sampler_desc.set_min_filter(web_sys::GpuFilterMode::Linear);
        sampler_desc.set_mag_filter(web_sys::GpuFilterMode::Linear);
        let sampler = device.create_sampler_with_descriptor(&sampler_desc);

        // Polygon storage buffers
        let polygon_meta_buf = create_storage_buffer(&device, MAX_POLYGONS, POLYGON_META_STRIDE)?;
        let polygon_vertex_buf = create_storage_buffer(
            &device,
            MAX_POLYGONS * MAX_POLYGON_VERTICES,
            VERTEX_STRIDE,
        )?;

        Ok(Self {
            device,
            queue,
            surface,
            pipelines,
            vertex_buf,
            index_buf,
            uniform_buf,
            sprite_instance_buf,
            rect_instance_buf,
            uniform_bg,
            texture_bgl: texture_bgl_layout,
            sampler,
            textures: TextureRegistry::new(),
            width,
            height,
            sprite_data: Vec::with_capacity(INSTANCE_CAPACITY * SPRITE_FLOATS),
            rect_data: Vec::with_capacity(INSTANCE_CAPACITY * RECT_FLOATS),
            sprite_tex_ids: Vec::with_capacity(INSTANCE_CAPACITY),
            polygon_meta_buf,
            polygon_vertex_buf,
            polygon_data: Vec::with_capacity(MAX_POLYGONS * 16), // 16 floats per meta
            polygon_vertices: Vec::with_capacity(MAX_POLYGONS * MAX_POLYGON_VERTICES * 2),
            debug_mode: 0,    // DEBUG_NONE
            output_gamma: 1.0,
            min_polygon_area_px: 1.0,
            edge_softness: 1.5,
        })
    }

    pub fn load_texture(
        &mut self,
        rgba_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<TextureHandle, JsValue> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| JsValue::from_str("texture dimensions overflow"))?;
        if rgba_data.len() != expected {
            return Err(JsValue::from_str(
                "RGBA data length does not match width * height * 4",
            ));
        }

        let mut extent = web_sys::GpuExtent3dDict::new(width);
        extent.set_height(height);
        let usage =
            web_sys::gpu_texture_usage::TEXTURE_BINDING | web_sys::gpu_texture_usage::COPY_DST;
        let desc = web_sys::GpuTextureDescriptor::new_with_gpu_extent_3d_dict(
            web_sys::GpuTextureFormat::Rgba8unorm,
            &extent,
            usage,
        );
        let texture = self.device.create_texture(&desc)?;

        let mut layout = web_sys::GpuTexelCopyBufferLayout::new();
        layout.set_bytes_per_row(width * 4);
        layout.set_rows_per_image(height);

        let copy = web_sys::GpuTexelCopyTextureInfo::new(&texture);

        let mut write_extent = web_sys::GpuExtent3dDict::new(width);
        write_extent.set_height(height);

        let _ = self
            .queue
            .write_texture_with_u8_slice_and_gpu_extent_3d_dict(
                &copy,
                rgba_data,
                &layout,
                &write_extent,
            );

        let id = self.textures.insert(texture, width, height)?;
        Ok(TextureHandle(id))
    }

    pub fn unload_texture(&mut self, handle: TextureHandle) {
        self.textures.unload(handle.0);
    }

    /// Whether `handle` currently refers to a live GPU texture.
    /// Used to reject `drawImage()` calls with a stale/unknown handle
    /// eagerly, instead of silently dropping the sprite at `present()`.
    pub fn has_texture(&self, handle: TextureHandle) -> bool {
        self.textures.has(handle.0)
    }

    // Global renderer configuration (from project assumptions point 2b)
    pub fn set_debug_mode(&mut self, mode: u32) {
        self.debug_mode = mode;
    }

    pub fn set_output_gamma(&mut self, gamma: f32) {
        self.output_gamma = gamma;
    }

    pub fn set_min_polygon_area_px(&mut self, area: f32) {
        self.min_polygon_area_px = area;
    }

    pub fn set_edge_softness(&mut self, softness: f32) {
        self.edge_softness = softness;
    }

    /// Upload and draw instances produced by `Renderer2D`.
    pub fn execute(&mut self, batch: &FrameBatch) -> Result<(), JsValue> {
        let clear_color = batch.clear.unwrap_or(Color::BLACK);
        self.sprite_data.clear();
        self.rect_data.clear();
        self.sprite_tex_ids.clear();
        self.polygon_data.clear();
        self.polygon_vertices.clear();

        for sprite in &batch.sprites {
            // Invalid handles are now rejected eagerly in `Antar::draw_image`.
            // This remains only as a defensive fallback for a texture that was
            // valid at `drawImage()` time but got `unloadTexture()`'d before
            // this `present()` — dropping that sprite is the right call.
            let Some(tex) = self.textures.get(sprite.texture_id) else {
                continue;
            };
            let tw = tex.width as f32;
            let th = tex.height as f32;
            if tw <= 0.0 || th <= 0.0 {
                continue;
            }
            let su = sprite.src.x / tw;
            let sv = sprite.src.y / th;
            let sw = sprite.src.w / tw;
            let sh = sprite.src.h / th;
            let t = &sprite.transform;

            self.sprite_data.extend_from_slice(&[
                sprite.dst.x,
                sprite.dst.y,
                sprite.dst.w,
                sprite.dst.h,
                su,
                sv,
                sw,
                sh,
                sprite.color.r,
                sprite.color.g,
                sprite.color.b,
                sprite.color.a * sprite.alpha,
                t.a,
                t.b,
                t.c,
                t.d,
                t.tx,
                t.ty,
            ]);
            self.sprite_tex_ids.push(sprite.texture_id);
        }

        for rect in &batch.rects {
            let t = &rect.transform;
            let final_a = rect.color.a * rect.alpha;
            self.rect_data.extend_from_slice(&[
                rect.rect.x,
                rect.rect.y,
                rect.rect.w,
                rect.rect.h,
                rect.color.r,
                rect.color.g,
                rect.color.b,
                final_a,
                t.a,
                t.b,
                t.c,
                t.d,
                t.tx,
                t.ty,
            ]);
        }

        // Process polygons for compute shader
        let mut vertex_offset = 0u32;
        for polygon in &batch.polygons {
            // Flatten metadata to f32 array (matches shader PolygonMeta layout)
            let meta = &polygon.meta;
            self.polygon_data.extend_from_slice(&[
                vertex_offset as f32, // Update vertex_offset in metadata
                meta.vertex_count as f32,
                meta.translate[0],
                meta.translate[1],
                meta.scale[0],
                meta.scale[1],
                meta.rotation,
                meta.render_mode as u32 as f32,
                meta.bbox_min[0],
                meta.bbox_min[1],
                meta.bbox_max[0],
                meta.bbox_max[1],
                meta.outline_width,
                meta.flags as f32,
                0.0, // padding
                0.0, // padding
                meta.tint_color.r,
                meta.tint_color.g,
                meta.tint_color.b,
                meta.tint_color.a,
            ]);

            // Flatten vertices
            for v in &polygon.vertices {
                self.polygon_vertices.push(v[0]);
                self.polygon_vertices.push(v[1]);
            }

            vertex_offset += polygon.meta.vertex_count;
        }

        let proj = ortho_matrix(self.width, self.height);
        let proj_js = Float32Array::from(proj.as_ref());
        let _ = self
            .queue
            .write_buffer_with_u32_and_buffer_source(&self.uniform_buf, 0, &proj_js);

        if !self.rect_data.is_empty() {
            let js = Float32Array::from(self.rect_data.as_slice());
            let _ = self
                .queue
                .write_buffer_with_u32_and_buffer_source(&self.rect_instance_buf, 0, &js);
        }
        if !self.sprite_data.is_empty() {
            let js = Float32Array::from(self.sprite_data.as_slice());
            let _ = self
                .queue
                .write_buffer_with_u32_and_buffer_source(&self.sprite_instance_buf, 0, &js);
        }

        // Upload polygon data if present
        if !self.polygon_data.is_empty() {
            let js = Float32Array::from(self.polygon_data.as_slice());
            let _ = self
                .queue
                .write_buffer_with_u32_and_buffer_source(&self.polygon_meta_buf, 0, &js);
        }
        if !self.polygon_vertices.is_empty() {
            let js = Float32Array::from(self.polygon_vertices.as_slice());
            let _ = self
                .queue
                .write_buffer_with_u32_and_buffer_source(&self.polygon_vertex_buf, 0, &js);
        }

        let encoder = self.device.create_command_encoder();
        let surface_view = encoder::current_texture_view(&self.surface)?;
        let mut pass = encoder::begin_render_pass(
            &encoder,
            &surface_view,
            clear_color.r as f64,
            clear_color.g as f64,
            clear_color.b as f64,
            clear_color.a as f64,
        )?;

        if !self.rect_data.is_empty() {
            pass.set_pipeline(&self.pipelines.rect);
            pass.set_bind_group(0, Some(&self.uniform_bg));
            pass.set_vertex_buffer(0, Some(&self.vertex_buf));
            pass.set_vertex_buffer(1, Some(&self.rect_instance_buf));
            pass.set_index_buffer(&self.index_buf, web_sys::GpuIndexFormat::Uint16);
            let n = (self.rect_data.len() / RECT_FLOATS) as u32;
            pass.draw_indexed_with_instance_count(6, n);
        }

        let mut i = 0usize;
        while i < self.sprite_tex_ids.len() {
            let tex_id = self.sprite_tex_ids[i];
            let start = i;
            while i < self.sprite_tex_ids.len() && self.sprite_tex_ids[i] == tex_id {
                i += 1;
            }
            let count = (i - start) as u32;

            if let Some(tex_entry) = self.textures.get(tex_id) {
                let view = tex_entry.texture.create_view()?;
                let tex_bg = bindgroups::create_texture_bg(
                    &self.device,
                    &self.texture_bgl,
                    &view,
                    &self.sampler,
                )?;

                let byte_offset = (start * SPRITE_INSTANCE_STRIDE) as f64;
                pass.set_pipeline(&self.pipelines.sprite);
                pass.set_bind_group(0, Some(&self.uniform_bg));
                pass.set_bind_group(1, Some(&tex_bg));
                pass.set_vertex_buffer(0, Some(&self.vertex_buf));
                pass.set_vertex_buffer_with_f64(1, Some(&self.sprite_instance_buf), byte_offset);
                pass.set_index_buffer(&self.index_buf, web_sys::GpuIndexFormat::Uint16);
                pass.draw_indexed_with_instance_count(6, count);
            }
        }

        pass.end();

        // Polygon compute pass execution
        // NOTE: Full compute shader execution requires web-sys with GpuComputePipeline support
        // Currently, polygon data is processed on CPU and uploaded to storage buffers,
        // but the actual GPU rasterization via compute shader is pending web-sys update.
        // The rasterizer.wgsl shader is ready and will be integrated when compute support is available.
        // For now, polygons are queued and processed but not rendered to screen.

        let cmd_buf = encoder.finish();
        self.queue.submit(&[cmd_buf]);

        Ok(())
    }
}
