use wasm_bindgen::prelude::*;

use super::command::{CommandQueue, DrawCommand, FrameBatch, TransformOp};
use super::params::{ImageDrawParams, PolygonDrawParams, RectDrawParams};
use super::state::StateStack;
use super::types::{Color, PolygonInstance, PolygonMeta, RectInstance, SpriteInstance, Transform};

/// CPU-side 2D renderer. Collects draw commands and resolves state at flush time.
/// Does not know about WebGPU — produces `SpriteInstance` / `RectInstance` only.
pub struct Renderer2D {
    commands: CommandQueue,
    /// Reused each frame — `clear_retain()` keeps heap allocation across `present()` calls.
    batch: FrameBatch,
}

impl Renderer2D {
    pub fn new() -> Self {
        Self {
            commands: CommandQueue::new(),
            batch: FrameBatch {
                clear: None,
                sprites: Vec::with_capacity(512), // Increased for better batching
                rects: Vec::with_capacity(128),
                polygons: Vec::with_capacity(64),
            },
        }
    }

    #[inline]
    pub fn push_command(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }

    /// Drain queued commands, resolve transforms/state, return GPU-ready instances.
    pub fn flush_commands(&mut self) -> &FrameBatch {
        let mut state = StateStack::new();
        self.batch.clear_retain();

        for cmd in self.commands.drain() {
            match cmd {
                DrawCommand::PushState => state.save(),
                DrawCommand::PopState => state.restore(),
                DrawCommand::Transform(op) => apply_transform(&mut state, op),
                DrawCommand::SetFillStyle(color) => state.set_fill_style_color(color),
                DrawCommand::SetAlpha(alpha) => state.set_global_alpha(alpha),
                DrawCommand::Clear(color) => {
                    self.batch.clear = Some(color);
                }
                DrawCommand::DrawImage(params) => {
                    self.batch.sprites.push(resolve_draw_image(&state, params));
                }
                DrawCommand::FillRect(params) => {
                    self.batch.rects.push(resolve_fill_rect(&state, params));
                }
                DrawCommand::FillPolygon(params) => {
                    if let Ok(instance) = resolve_fill_polygon(&state, params) {
                        self.batch.polygons.push(instance);
                    }
                }
            }
        }

        &self.batch
    }
}

fn apply_transform(state: &mut StateStack, op: TransformOp) {
    match op {
        TransformOp::Translate(x, y) => state.translate(x, y),
        TransformOp::Rotate(angle) => state.rotate(angle),
        TransformOp::Scale(sx, sy) => state.scale(sx, sy),
    }
}

fn resolve_draw_image(state: &StateStack, params: ImageDrawParams) -> SpriteInstance {
    let s = state.current();
    let mut transform = s.transform;

    if params.rotation != 0.0 || params.flip_x || params.flip_y {
        let cx = params.dst.x + params.dst.w * 0.5;
        let cy = params.dst.y + params.dst.h * 0.5;
        let to_origin = Transform::translate(-cx, -cy);
        let back = Transform::translate(cx, cy);

        // `local` folds together everything that must happen "in place",
        // around the sprite's own center, before being sandwiched between
        // to_origin/back below.
        let mut local = Transform::IDENTITY;
        if params.flip_x || params.flip_y {
            let sx = if params.flip_x { -1.0 } else { 1.0 };
            let sy = if params.flip_y { -1.0 } else { 1.0 };
            local = local.concat(&Transform::scale(sx, sy));
        }
        if params.rotation != 0.0 {
            local = local.concat(&Transform::rotate(params.rotation));
        }

        // self.concat(&other) computes self*other, which applied to a point v
        // is self*(other*v) — `other` acts first (inner), `self` second
        // (outer). We want: v -> to_origin*v (move center to 0) -> local*that
        // (rotate/flip in place) -> back*that (move center back). As a single
        // matrix that's `back * local * to_origin`, built left-to-right as
        // `back.concat(&local).concat(&to_origin)`.
        //
        // The previous version had this backwards —
        // `to_origin.concat(&local).concat(&back)` — which computes
        // `to_origin * local * back`, applying `back` FIRST and `to_origin`
        // LAST. For rotation-only this was visually subtle (a rotation around
        // the wrong pivot still roughly stays in frame), but for flip the
        // wrong-pivot mirror threw the sprite far outside dst.
        transform = transform.concat(&back.concat(&local).concat(&to_origin));
    }

    let mut color = params.color;
    color.a *= s.global_alpha;

    SpriteInstance {
        texture_id: params.texture.0,
        src: params.src,
        dst: params.dst,
        transform,
        color,
        alpha: s.global_alpha,
    }
}

fn resolve_fill_rect(state: &StateStack, params: RectDrawParams) -> RectInstance {
    let s = state.current();
    let color = if params.color == Color::WHITE {
        s.fill_style
    } else {
        params.color
    };

    RectInstance {
        rect: params.rect,
        color,
        transform: s.transform,
        alpha: s.global_alpha,
    }
}

/// Count vertices and reject degenerate polygons.
///
/// Kept free of `JsValue` on purpose: constructing one panics outside wasm32,
/// so folding the check into the `Result<_, JsValue>` would make it untestable
/// on the host.
fn checked_vertex_count(points: &[f32]) -> Result<usize, &'static str> {
    let count = points.len() / 2;
    if count < 3 {
        return Err("polygon must have at least 3 vertices");
    }
    Ok(count)
}

fn resolve_fill_polygon(
    state: &StateStack,
    params: PolygonDrawParams,
) -> Result<PolygonInstance, JsValue> {
    let s = state.current();

    // Parse flat points array into vertices
    let vertex_count = checked_vertex_count(&params.points).map_err(JsValue::from_str)?;

    // Transform vertices on CPU (per design: geometry transformed once, not per-pixel)
    // Optimized: pre-extract transform components to avoid repeated field access
    let t = &s.transform;
    let (a, b, c, d, tx, ty) = (t.a, t.b, t.c, t.d, t.tx, t.ty);

    let mut vertices = Vec::with_capacity(vertex_count);
    let mut bbox_min = [f32::MAX, f32::MAX];
    let mut bbox_max = [f32::MIN, f32::MIN];

    // Optimized vertex transformation loop
    let points = &params.points;
    for i in 0..vertex_count {
        let x = points[i * 2];
        let y = points[i * 2 + 1];

        // Apply transform: [a c tx; b d ty; 0 0 1] * [x y 1]
        let transformed_x = a * x + c * y + tx;
        let transformed_y = b * x + d * y + ty;

        vertices.push([transformed_x, transformed_y]);

        // Update bounding box (min/max operations are fast)
        bbox_min[0] = bbox_min[0].min(transformed_x);
        bbox_min[1] = bbox_min[1].min(transformed_y);
        bbox_max[0] = bbox_max[0].max(transformed_x);
        bbox_max[1] = bbox_max[1].max(transformed_y);
    }

    // Build metadata matching shader PolygonMeta structure
    let mut color = params.color;
    color.a *= s.global_alpha;

    let flags = if params.skip_if_transparent {
        1 // FLAG_SKIP_TRANSPARENT
    } else {
        0
    };

    let meta = PolygonMeta {
        vertex_offset: 0, // Will be set by GPU context when batching
        vertex_count: vertex_count as u32,
        translate: [0.0, 0.0], // Already applied to vertices
        scale: [1.0, 1.0],     // Already applied to vertices
        rotation: 0.0,         // Already applied to vertices
        render_mode: params.render_mode,
        bbox_min,
        bbox_max,
        outline_width: params.outline_width,
        flags,
        tint_color: color,
    };

    Ok(PolygonInstance {
        meta,
        vertices,
        texture_id: params.texture,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::types::PolygonRenderMode;

    fn params(points: Vec<f32>) -> PolygonDrawParams {
        PolygonDrawParams {
            points,
            color: Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            render_mode: PolygonRenderMode::Fill,
            outline_width: 0.0,
            texture: None,
            skip_if_transparent: false,
        }
    }

    fn triangle() -> Vec<f32> {
        vec![10.0, 20.0, 110.0, 20.0, 60.0, 220.0]
    }

    #[test]
    fn rejects_polygons_with_fewer_than_three_vertices() {
        // Two vertices, and an empty point list.
        assert!(checked_vertex_count(&[0.0, 0.0, 1.0, 1.0]).is_err());
        assert!(checked_vertex_count(&[]).is_err());

        // An odd-length list truncates to two vertices, so it must also fail.
        assert!(checked_vertex_count(&[0.0, 0.0, 1.0, 1.0, 2.0]).is_err());
    }

    #[test]
    fn accepts_three_or_more_vertices() {
        assert_eq!(checked_vertex_count(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0]), Ok(3));
        let many: Vec<f32> = (0..20).map(|i| i as f32).collect();
        assert_eq!(checked_vertex_count(&many), Ok(10));
    }

    #[test]
    fn passes_vertices_through_and_derives_the_bbox_when_untransformed() {
        let state = StateStack::new();
        let polygon = resolve_fill_polygon(&state, params(triangle())).unwrap();

        assert_eq!(polygon.meta.vertex_count, 3);
        assert_eq!(
            polygon.vertices,
            vec![[10.0, 20.0], [110.0, 20.0], [60.0, 220.0]]
        );
        assert_eq!(polygon.meta.bbox_min, [10.0, 20.0]);
        assert_eq!(polygon.meta.bbox_max, [110.0, 220.0]);

        // The transform is folded into the vertices, so the metadata must
        // carry identity — the shader relies on this and does not re-apply it.
        assert_eq!(polygon.meta.translate, [0.0, 0.0]);
        assert_eq!(polygon.meta.scale, [1.0, 1.0]);
        assert_eq!(polygon.meta.rotation, 0.0);
    }

    #[test]
    fn applies_the_current_transform_to_vertices_and_bbox() {
        let mut state = StateStack::new();
        state.translate(5.0, -3.0);

        let polygon =
            resolve_fill_polygon(&state, params(vec![0.0, 0.0, 10.0, 0.0, 0.0, 10.0])).unwrap();

        assert_eq!(polygon.vertices[0], [5.0, -3.0]);
        assert_eq!(polygon.vertices[1], [15.0, -3.0]);
        assert_eq!(polygon.vertices[2], [5.0, 7.0]);
        assert_eq!(polygon.meta.bbox_min, [5.0, -3.0]);
        assert_eq!(polygon.meta.bbox_max, [15.0, 7.0]);
    }

    #[test]
    fn bbox_covers_every_transformed_vertex() {
        let mut state = StateStack::new();
        state.translate(100.0, 100.0);
        state.scale(2.0, 3.0);

        let polygon =
            resolve_fill_polygon(&state, params(vec![-4.0, -5.0, 6.0, -5.0, -4.0, 7.0])).unwrap();

        for v in &polygon.vertices {
            assert!(v[0] >= polygon.meta.bbox_min[0]);
            assert!(v[1] >= polygon.meta.bbox_min[1]);
            assert!(v[0] <= polygon.meta.bbox_max[0]);
            assert!(v[1] <= polygon.meta.bbox_max[1]);
        }
        assert_eq!(polygon.meta.bbox_min, [92.0, 85.0]);
        assert_eq!(polygon.meta.bbox_max, [112.0, 121.0]);
    }

    #[test]
    fn folds_global_alpha_into_the_tint() {
        let mut state = StateStack::new();
        state.set_global_alpha(0.5);

        let polygon = resolve_fill_polygon(&state, params(triangle())).unwrap();

        assert_eq!(polygon.meta.tint_color.a, 0.5);
        assert_eq!(polygon.meta.tint_color.r, 1.0);
    }

    #[test]
    fn records_the_outline_and_skip_flags() {
        let state = StateStack::new();
        let mut p = params(triangle());
        p.outline_width = 6.0;
        p.skip_if_transparent = true;
        p.render_mode = PolygonRenderMode::Outline;

        let polygon = resolve_fill_polygon(&state, p).unwrap();

        assert_eq!(polygon.meta.outline_width, 6.0);
        assert_eq!(polygon.meta.flags, 1);
        assert_eq!(polygon.meta.render_mode, PolygonRenderMode::Outline);
    }
}
