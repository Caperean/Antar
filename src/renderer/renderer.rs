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
                sprites: Vec::with_capacity(256),
                rects: Vec::with_capacity(64),
                polygons: Vec::with_capacity(32),
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

fn resolve_fill_polygon(state: &StateStack, params: PolygonDrawParams) -> Result<PolygonInstance, JsValue> {
    let s = state.current();

    // Parse flat points array into vertices
    let vertex_count = params.points.len() / 2;
    if vertex_count < 3 {
        return Err(JsValue::from_str("polygon must have at least 3 vertices"));
    }

    // Transform vertices on CPU (per design: geometry transformed once, not per-pixel)
    let mut vertices = Vec::with_capacity(vertex_count);
    let mut bbox_min = [f32::MAX, f32::MAX];
    let mut bbox_max = [f32::MIN, f32::MIN];

    let t = &s.transform;
    for i in 0..vertex_count {
        let x = params.points[i * 2];
        let y = params.points[i * 2 + 1];

        // Apply transform: [a c tx; b d ty; 0 0 1] * [x y 1]
        let tx = t.a * x + t.c * y + t.tx;
        let ty = t.b * x + t.d * y + t.ty;

        vertices.push([tx, ty]);

        // Update bounding box
        bbox_min[0] = bbox_min[0].min(tx);
        bbox_min[1] = bbox_min[1].min(ty);
        bbox_max[0] = bbox_max[0].max(tx);
        bbox_max[1] = bbox_max[1].max(ty);
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
