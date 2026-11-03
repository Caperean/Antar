pub mod bindgroups;
pub mod buffers;
pub mod device;
pub mod encoder;
pub mod pipeline;
pub mod surface;
pub mod texture;

use js_sys::Float32Array;
use wasm_bindgen::prelude::*;
use web_sys::{GpuBindGroup, GpuBuffer, GpuCanvasContext, GpuDevice, GpuQueue, HtmlCanvasElement};

use crate::renderer::{
    command::FrameBatch,
    types::{Color, PolygonInstance, TextureHandle},
};
use bindgroups::{create_polygon_bg, create_uniform_bg, polygon_bgl, texture_bgl, uniform_bgl};
use buffers::{
    create_index_buffer, create_instance_buffer, create_storage_buffer, create_uniform_buffer,
    create_vertex_buffer, ortho_matrix,
};
use device::install_error_reporting;
use pipeline::Pipelines;
use surface::{configure_surface, reconfigure};
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
/// Matches `PolygonMeta` in `shaders/polygon.wgsl`: 24 × f32 = 96 bytes.
/// The previous value of 64 undersized the buffer — the flatten in `execute`
/// writes 96 bytes per polygon, so past 409 polygons the upload would have
/// run past the end of the buffer and failed validation.
const POLYGON_META_STRIDE: usize = 96;
const POLYGON_META_FLOATS: usize = POLYGON_META_STRIDE / 4;
const VERTEX_STRIDE: usize = 8; // vec2<f32> = 2 floats × 4 bytes

/// Per-frame polygon settings. Replicated into every instance so the polygon
/// shader needs no uniform binding of its own.
#[derive(Clone, Copy)]
struct PolygonGlobals {
    edge_softness: f32,
    debug_mode: u32,
    output_gamma: f32,
    min_polygon_area_px: f32,
}

/// Pack one polygon into the shader's `PolygonMeta` layout.
///
/// The return type is fixed at `POLYGON_META_FLOATS`, so the array literal below
/// failing to hold exactly that many values is a compile error rather than a
/// buffer overrun discovered at runtime.
fn flatten_polygon_meta(
    polygon: &PolygonInstance,
    vertex_offset: u32,
    globals: PolygonGlobals,
) -> [f32; POLYGON_META_FLOATS] {
    let meta = &polygon.meta;
    [
        vertex_offset as f32,
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
        globals.edge_softness,
        globals.debug_mode as f32,
        globals.output_gamma,
        globals.min_polygon_area_px,
    ]
}

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
    polygon_data: Vec<f32>,     // Flattened polygon metadata
    polygon_vertices: Vec<f32>, // Flattened vertex coordinates

    // Global renderer configuration (from zalozenia.txt point 2b)
    debug_mode: u32,
    output_gamma: f32,
    min_polygon_area_px: f32,
    edge_softness: f32,

    /// Re-read every frame: a window resize or DPR change invalidates the
    /// stored dimensions, and the projection matrix is built from them.
    canvas: HtmlCanvasElement,
    polygon_bg: GpuBindGroup,
    /// 1×1 opaque white, bound for polygons that carry no texture so the
    /// shader's sampling binding is always valid. The bind group holds a
    /// reference that keeps the underlying texture alive.
    dummy_bg: GpuBindGroup,
}

impl GpuContext {
    pub async fn init(canvas: &HtmlCanvasElement) -> Result<Self, JsValue> {
        let (_adapter, device, queue) = device::init_device().await?;

        // Installed before anything is built on the device. Pipeline and
        // bind-group validation errors do not throw — they arrive later, via
        // this handler — so it has to be in place before the first one runs.
        install_error_reporting(&device);

        let width = canvas.width();
        let height = canvas.height();
        let surface = configure_surface(canvas, &device)?;

        let uniform_bgl = uniform_bgl(&device)?;
        let texture_bgl_layout = texture_bgl(&device)?;
        let polygon_bgl_layout = polygon_bgl(&device)?;
        let pipelines = Pipelines::new(
            &device,
            &uniform_bgl,
            &texture_bgl_layout,
            &polygon_bgl_layout,
        )?;

        let vertex_buf = create_vertex_buffer(&device)?;
        let index_buf = create_index_buffer(&device)?;
        let uniform_buf = create_uniform_buffer(&device)?;
        let sprite_instance_buf =
            create_instance_buffer(&device, INSTANCE_CAPACITY, SPRITE_INSTANCE_STRIDE)?;
        let rect_instance_buf =
            create_instance_buffer(&device, INSTANCE_CAPACITY, RECT_INSTANCE_STRIDE)?;
        let uniform_bg = create_uniform_bg(&device, &uniform_bgl, &uniform_buf, 64)?;

        let sampler_desc = web_sys::GpuSamplerDescriptor::new();
        sampler_desc.set_min_filter(web_sys::GpuFilterMode::Linear);
        sampler_desc.set_mag_filter(web_sys::GpuFilterMode::Linear);
        let sampler = device.create_sampler_with_descriptor(&sampler_desc);

        // Polygon storage buffers
        let polygon_meta_buf = create_storage_buffer(&device, MAX_POLYGONS, POLYGON_META_STRIDE)?;
        let polygon_vertex_buf =
            create_storage_buffer(&device, MAX_POLYGONS * MAX_POLYGON_VERTICES, VERTEX_STRIDE)?;
        let polygon_bg = create_polygon_bg(
            &device,
            &polygon_bgl_layout,
            &polygon_meta_buf,
            &polygon_vertex_buf,
        )?;

        let dummy_bg = bindgroups::create_texture_bg(
            &device,
            &texture_bgl_layout,
            &create_dummy_texture(&device, &queue)?.create_view()?,
            &sampler,
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
            polygon_data: Vec::with_capacity(MAX_POLYGONS * POLYGON_META_FLOATS),
            polygon_vertices: Vec::with_capacity(MAX_POLYGONS * MAX_POLYGON_VERTICES * 2),
            debug_mode: 0, // DEBUG_NONE
            output_gamma: 1.0,
            min_polygon_area_px: 1.0,
            edge_softness: 1.5,
            canvas: canvas.clone(),
            polygon_bg,
            dummy_bg,
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

        let extent = web_sys::GpuExtent3dDict::new(width);
        extent.set_height(height);
        let usage =
            web_sys::gpu_texture_usage::TEXTURE_BINDING | web_sys::gpu_texture_usage::COPY_DST;
        let desc = web_sys::GpuTextureDescriptor::new_with_gpu_extent_3d_dict(
            web_sys::GpuTextureFormat::Rgba8unorm,
            &extent,
            usage,
        );
        let texture = self.device.create_texture(&desc)?;

        let layout = web_sys::GpuTexelCopyBufferLayout::new();
        layout.set_bytes_per_row(width * 4);
        layout.set_rows_per_image(height);

        let copy = web_sys::GpuTexelCopyTextureInfo::new(&texture);

        let write_extent = web_sys::GpuExtent3dDict::new(width);
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
        // The canvas can change size after init (window resize, DPR change).
        // The projection matrix is built from the stored dimensions, so a stale
        // size stretches and misplaces every draw even though the surface itself
        // would follow the canvas.
        let (canvas_w, canvas_h) = (self.canvas.width(), self.canvas.height());
        if canvas_w != self.width || canvas_h != self.height {
            self.resize(canvas_w, canvas_h)?;
        }
        if self.width == 0 || self.height == 0 {
            // Minimised, or the display went away: there is no surface to draw
            // into, and asking for one would be an error.
            return Ok(());
        }

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

        let globals = PolygonGlobals {
            edge_softness: self.edge_softness,
            debug_mode: self.debug_mode,
            output_gamma: self.output_gamma,
            min_polygon_area_px: self.min_polygon_area_px,
        };

        // Process polygons for the fragment shader
        let mut vertex_offset = 0u32;
        for polygon in &batch.polygons {
            self.polygon_data.extend_from_slice(&flatten_polygon_meta(
                polygon,
                vertex_offset,
                globals,
            ));

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
            let _ =
                self.queue
                    .write_buffer_with_u32_and_buffer_source(&self.rect_instance_buf, 0, &js);
        }
        if !self.sprite_data.is_empty() {
            let js = Float32Array::from(self.sprite_data.as_slice());
            let _ = self.queue.write_buffer_with_u32_and_buffer_source(
                &self.sprite_instance_buf,
                0,
                &js,
            );
        }

        // Upload polygon data if present
        if !self.polygon_data.is_empty() {
            let js = Float32Array::from(self.polygon_data.as_slice());
            let _ =
                self.queue
                    .write_buffer_with_u32_and_buffer_source(&self.polygon_meta_buf, 0, &js);
        }
        if !self.polygon_vertices.is_empty() {
            let js = Float32Array::from(self.polygon_vertices.as_slice());
            let _ = self.queue.write_buffer_with_u32_and_buffer_source(
                &self.polygon_vertex_buf,
                0,
                &js,
            );
        }

        let encoder = self.device.create_command_encoder();
        let surface_view = encoder::current_texture_view(&self.surface)?;
        let pass = encoder::begin_render_pass(
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

        // Polygons: one quad per polygon, sized to its bounding box by the
        // vertex shader, with the even-odd fill evaluated per pixel.
        //
        // Draw order note: `FrameBatch` groups by type, so rects, sprites and
        // polygons each render as a block rather than in original command
        // order. That is pre-existing behaviour, not new here — polygons are
        // drawn last so they are not hidden behind the other two.
        if !self.polygon_data.is_empty() {
            pass.set_pipeline(&self.pipelines.polygon);
            pass.set_bind_group(0, Some(&self.uniform_bg));
            pass.set_bind_group(1, Some(&self.polygon_bg));
            pass.set_vertex_buffer(0, Some(&self.vertex_buf));
            pass.set_index_buffer(&self.index_buf, web_sys::GpuIndexFormat::Uint16);

            // Consecutive polygons sharing a texture become one draw call.
            // `first_instance` shifts which metadata slot each instance reads,
            // so batching does not disturb per-polygon data.
            let mut run_start = 0usize;
            while run_start < batch.polygons.len() {
                let texture_id = batch.polygons[run_start].texture_id;
                let mut run_end = run_start + 1;
                while run_end < batch.polygons.len()
                    && batch.polygons[run_end].texture_id == texture_id
                {
                    run_end += 1;
                }

                // A texture unloaded between drawing and present() falls back
                // to white rather than dropping the polygon outright.
                let bg = match texture_id.and_then(|id| self.textures.get(id.0)) {
                    Some(tex) => bindgroups::create_texture_bg(
                        &self.device,
                        &self.texture_bgl,
                        &tex.texture.create_view()?,
                        &self.sampler,
                    )?,
                    None => self.dummy_bg.clone(),
                };
                pass.set_bind_group(2, Some(&bg));

                let count = (run_end - run_start) as u32;
                pass
                    .draw_indexed_with_instance_count_and_first_index_and_base_vertex_and_first_instance(
                        6,
                        count,
                        0,
                        0,
                        run_start as u32,
                    );

                run_start = run_end;
            }
        }

        pass.end();

        let cmd_buf = encoder.finish();
        self.queue.submit(&[cmd_buf]);

        Ok(())
    }

    /// Bring the stored surface dimensions back in line with the canvas.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), JsValue> {
        if width == self.width && height == self.height {
            return Ok(());
        }
        self.width = width;
        self.height = height;

        if width == 0 || height == 0 {
            // Nothing to configure; `execute` skips the frame entirely.
            return Ok(());
        }

        reconfigure(&self.device, &self.surface)
    }
}

/// A 1×1 opaque white texture: the default sample for polygons that carry no
/// texture, so the shader always has a valid binding at group 2.
fn create_dummy_texture(
    device: &GpuDevice,
    queue: &GpuQueue,
) -> Result<web_sys::GpuTexture, JsValue> {
    let usage = web_sys::gpu_texture_usage::TEXTURE_BINDING | web_sys::gpu_texture_usage::COPY_DST;

    let size = web_sys::GpuExtent3dDict::new(1);
    size.set_height(1);
    let desc = web_sys::GpuTextureDescriptor::new_with_gpu_extent_3d_dict(
        web_sys::GpuTextureFormat::Rgba8unorm,
        &size,
        usage,
    );
    let texture = device.create_texture(&desc)?;

    let layout = web_sys::GpuTexelCopyBufferLayout::new();
    layout.set_bytes_per_row(4);
    layout.set_rows_per_image(1);

    let copy = web_sys::GpuTexelCopyTextureInfo::new(&texture);
    let write_size = web_sys::GpuExtent3dDict::new(1);
    write_size.set_height(1);
    queue.write_texture_with_u8_slice_and_gpu_extent_3d_dict(
        &copy,
        &[255, 255, 255, 255],
        &layout,
        &write_size,
    )?;

    Ok(texture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::types::{PolygonMeta, PolygonRenderMode};

    const GLOBALS: PolygonGlobals = PolygonGlobals {
        edge_softness: 1.5,
        debug_mode: 2,
        output_gamma: 2.2,
        min_polygon_area_px: 1.0,
    };

    fn sample_polygon() -> PolygonInstance {
        PolygonInstance {
            meta: PolygonMeta {
                vertex_offset: 0,
                vertex_count: 3,
                translate: [0.0, 0.0],
                scale: [1.0, 1.0],
                rotation: 0.0,
                render_mode: PolygonRenderMode::Fill,
                bbox_min: [10.0, 20.0],
                bbox_max: [110.0, 220.0],
                outline_width: 4.0,
                flags: 1,
                tint_color: Color {
                    r: 1.0,
                    g: 0.5,
                    b: 0.25,
                    a: 0.8,
                },
            },
            vertices: vec![[10.0, 20.0], [110.0, 20.0], [60.0, 220.0]],
            texture_id: None,
        }
    }

    /// The shader's `PolygonMeta` occupies 96 bytes, and an array element
    /// stride in a storage buffer must be a multiple of 16. Getting this wrong
    /// previously undersized the buffer, which only failed past ~409 polygons.
    #[test]
    fn meta_stride_matches_the_shader_struct() {
        assert_eq!(POLYGON_META_FLOATS, 24);
        assert_eq!(POLYGON_META_STRIDE, POLYGON_META_FLOATS * 4);
        assert_eq!(POLYGON_META_STRIDE, 96);
        assert_eq!(POLYGON_META_STRIDE % 16, 0);
    }

    #[test]
    fn vertex_stride_matches_the_shader_vec2() {
        assert_eq!(VERTEX_STRIDE, 8);
        assert_eq!(VERTEX_STRIDE % 4, 0);
    }

    #[test]
    fn flatten_lays_out_fields_where_the_shader_expects_them() {
        let flat = flatten_polygon_meta(&sample_polygon(), 7, GLOBALS);

        assert_eq!(flat.len(), POLYGON_META_FLOATS);
        assert_eq!(flat[0], 7.0); // vertex_offset (running batch offset)
        assert_eq!(flat[1], 3.0); // vertex_count
        assert_eq!(flat[2], 0.0); // translate.x — identity
        assert_eq!(flat[3], 0.0); // translate.y
        assert_eq!(flat[4], 1.0); // scale.x — identity
        assert_eq!(flat[5], 1.0); // scale.y
        assert_eq!(flat[6], 0.0); // rotation — identity
        assert_eq!(flat[7], 0.0); // render_mode = Fill
        assert_eq!(flat[8], 10.0); // bbox_min.x
        assert_eq!(flat[9], 20.0); // bbox_min.y
        assert_eq!(flat[10], 110.0); // bbox_max.x
        assert_eq!(flat[11], 220.0); // bbox_max.y
        assert_eq!(flat[12], 4.0); // outline_width
        assert_eq!(flat[13], 1.0); // flags
        assert_eq!(flat[14], 0.0); // padding
        assert_eq!(flat[15], 0.0); // padding
        assert_eq!(flat[16], 1.0); // tint.r
        assert_eq!(flat[17], 0.5); // tint.g
        assert_eq!(flat[18], 0.25); // tint.b
        assert_eq!(flat[19], 0.8); // tint.a
        assert_eq!(flat[20], 1.5); // edge_softness
        assert_eq!(flat[21], 2.0); // debug_mode
        assert_eq!(flat[22], 2.2); // output_gamma
        assert_eq!(flat[23], 1.0); // min_polygon_area_px
    }
}
