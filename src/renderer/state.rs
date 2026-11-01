use super::types::{Color, Transform};

/// The mutable drawing state that mirrors the Canvas2D state machine.
/// Pushed/popped via `save()` / `restore()`.
#[derive(Clone, Debug)]
pub struct DrawState {
    pub transform: Transform,
    pub fill_style: Color,
    pub global_alpha: f32,
}

impl Default for DrawState {
    fn default() -> Self {
        Self {
            transform: Transform::IDENTITY,
            fill_style: Color::BLACK,
            global_alpha: 1.0,
        }
    }
}

/// Manages the current drawing state and a save/restore stack.
pub struct StateStack {
    current: DrawState,
    stack: Vec<DrawState>,
}

impl StateStack {
    pub fn new() -> Self {
        Self {
            current: DrawState::default(),
            stack: Vec::with_capacity(32),
        }
    }

    // ── Save / Restore ──────────────────────────────────────────────────────

    pub fn save(&mut self) {
        self.stack.push(self.current.clone());
    }

    pub fn restore(&mut self) {
        if let Some(prev) = self.stack.pop() {
            self.current = prev;
        }
    }

    // ── Transforms ──────────────────────────────────────────────────────────

    pub fn translate(&mut self, x: f32, y: f32) {
        let t = Transform::translate(x, y);
        self.current.transform = self.current.transform.concat(&t);
    }

    pub fn rotate(&mut self, radians: f32) {
        let r = Transform::rotate(radians);
        self.current.transform = self.current.transform.concat(&r);
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        let s = Transform::scale(sx, sy);
        self.current.transform = self.current.transform.concat(&s);
    }

    // ── Drawing state ────────────────────────────────────────────────────────

    pub fn set_fill_style_color(&mut self, color: Color) {
        self.current.fill_style = color;
    }

    pub fn set_global_alpha(&mut self, alpha: f32) {
        if alpha.is_finite() {
            self.current.global_alpha = alpha.clamp(0.0, 1.0);
        }
    }

    // ── Accessors ────────────────────────────────────────────────────────────

    #[inline]
    pub fn current(&self) -> &DrawState {
        &self.current
    }

    /// Reset state (called between frames if needed).
    /// `flush_commands` builds a fresh stack per frame today, so nothing calls this.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.stack.clear();
        self.current = DrawState::default();
    }
}
