use super::params::{ImageDrawParams, PolygonDrawParams, RectDrawParams};
use super::types::{Color, PolygonInstance, RectInstance, SpriteInstance};

/// High-level draw commands recorded by the JS API.
/// Processed by `Renderer2D` at `present()` time.
#[derive(Clone, Debug)]
pub enum DrawCommand {
    Clear(Color),
    DrawImage(ImageDrawParams),
    FillRect(RectDrawParams),
    FillPolygon(PolygonDrawParams),
    SetFillStyle(Color),
    SetAlpha(f32),
    Transform(TransformOp),
    PushState,
    PopState,
}

#[derive(Clone, Copy, Debug)]
pub enum TransformOp {
    Translate(f32, f32),
    Rotate(f32),
    Scale(f32, f32),
}

/// GPU-ready frame produced by `Renderer2D` after resolving state/transforms.
#[derive(Clone, Debug, Default)]
pub struct FrameBatch {
    pub clear: Option<Color>,
    pub sprites: Vec<SpriteInstance>,
    pub rects: Vec<RectInstance>,
    pub polygons: Vec<PolygonInstance>,
}

impl FrameBatch {
    pub fn clear_retain(&mut self) {
        self.clear = None;
        self.sprites.clear();
        self.rects.clear();
        self.polygons.clear();
    }
}

/// Allocation-efficient per-frame command queue.
pub struct CommandQueue {
    commands: Vec<DrawCommand>,
}

impl CommandQueue {
    pub fn new() -> Self {
        Self {
            commands: Vec::with_capacity(2048), // Increased for better batching
        }
    }

    #[inline]
    pub fn push(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }

    #[inline]
    pub fn drain(&mut self) -> std::vec::Drain<'_, DrawCommand> {
        self.commands.drain(..)
    }
}
