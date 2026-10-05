use wasm_bindgen::prelude::*;

use super::types::{finite_f32, finite_f32_field, Color, PolygonRenderMode, Rect, TextureHandle};

/// Parameters for `drawImage`, parsed from a JS object.
#[derive(Clone, Debug)]
pub struct ImageDrawParams {
    pub texture: TextureHandle,
    pub src: Rect,
    pub dst: Rect,
    pub rotation: f32,
    pub color: Color,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// Parameters for `fillRect`, parsed from a JS object.
#[derive(Clone, Debug)]
pub struct RectDrawParams {
    pub rect: Rect,
    pub color: Color,
}

/// Parameters for `fillPolygon`, parsed from a JS object.
#[derive(Clone, Debug)]
pub struct PolygonDrawParams {
    pub points: Vec<f32>, // Flat list: [x0, y0, x1, y1, ...]
    pub color: Color,
    pub render_mode: PolygonRenderMode,
    pub outline_width: f32,
    pub texture: Option<TextureHandle>,
    pub skip_if_transparent: bool,
}

impl ImageDrawParams {
    pub fn from_js(js: &JsValue) -> Result<Self, JsValue> {
        let texture = finite_f32_field(js, "texture")? as u32;
        let src = rect_from(js, "src")?;
        let dst = rect_from(js, "dst")?;
        let rotation = optional_finite_f32(js, "rotation")?.unwrap_or(0.0);
        let color = color_from(js, "color").unwrap_or(Color::WHITE);
        let flip_x = bool_from(js, "flipX").unwrap_or(false);
        let flip_y = bool_from(js, "flipY").unwrap_or(false);

        Ok(Self {
            texture: TextureHandle(texture),
            src,
            dst,
            rotation,
            color,
            flip_x,
            flip_y,
        })
    }
}

impl RectDrawParams {
    pub fn from_js(js: &JsValue) -> Result<Self, JsValue> {
        let rect = rect_from(js, "rect")?;
        let color = color_from(js, "color").unwrap_or(Color::WHITE);
        Ok(Self { rect, color })
    }
}

impl PolygonDrawParams {
    pub fn from_js(js: &JsValue) -> Result<Self, JsValue> {
        let points_arr = js_sys::Reflect::get(js, &"points".into())?;
        if points_arr.is_undefined() || points_arr.is_null() {
            return Err(JsValue::from_str("missing field: points"));
        }

        let points = if let Some(arr) = points_arr.dyn_ref::<js_sys::Array>() {
            let mut pts = Vec::with_capacity(arr.length() as usize);
            for i in 0..arr.length() {
                let val = arr.get(i);
                let n = val
                    .as_f64()
                    .ok_or_else(|| JsValue::from_str("points must be numbers"))?;
                pts.push(finite_f32(n, "points")?);
            }
            pts
        } else {
            return Err(JsValue::from_str("points must be an array"));
        };

        if points.len() < 6 {
            return Err(JsValue::from_str(
                "points must have at least 3 vertices (6 numbers)",
            ));
        }
        if points.len() % 2 != 0 {
            return Err(JsValue::from_str("points must have even length (x,y pairs)"));
        }

        let color = color_from(js, "color").unwrap_or(Color::WHITE);

        let render_mode_str = optional_string_field(js, "renderMode").unwrap_or(None);
        let render_mode = match render_mode_str.as_deref() {
            Some("outline") => PolygonRenderMode::Outline,
            Some("mask") => PolygonRenderMode::Mask,
            Some("fill") | None => PolygonRenderMode::Fill,
            Some(other) => {
                return Err(JsValue::from_str(&format!(
                    "invalid renderMode: {other} (expected 'fill', 'outline', or 'mask')"
                )))
            }
        };

        let outline_width = optional_finite_f32(js, "outlineWidth")?.unwrap_or(4.0);

        let texture = if let Some(tex_val) = optional_field(js, "texture")? {
            let tex_id = tex_val
                .as_f64()
                .ok_or_else(|| JsValue::from_str("texture must be a number"))? as u32;
            Some(TextureHandle(tex_id))
        } else {
            None
        };

        let skip_if_transparent = bool_from(js, "skipIfTransparent").unwrap_or(false);

        Ok(Self {
            points,
            color,
            render_mode,
            outline_width,
            texture,
            skip_if_transparent,
        })
    }
}

fn rect_from(obj: &JsValue, key: &str) -> Result<Rect, JsValue> {
    let v = js_sys::Reflect::get(obj, &key.into())?;
    if v.is_undefined() || v.is_null() {
        return Err(JsValue::from_str(&format!("missing field: {key}")));
    }
    Rect::from_js(&v)
}

fn color_from(obj: &JsValue, key: &str) -> Result<Color, JsValue> {
    let v = js_sys::Reflect::get(obj, &key.into())?;
    if v.is_undefined() || v.is_null() {
        return Err(JsValue::from_str(&format!("missing field: {key}")));
    }
    Color::from_js(&v)
}

fn optional_finite_f32(obj: &JsValue, key: &str) -> Result<Option<f32>, JsValue> {
    let v = js_sys::Reflect::get(obj, &key.into())?;
    if v.is_undefined() || v.is_null() {
        // Field genuinely absent — caller applies its own default.
        return Ok(None);
    }
    let n = v
        .as_f64()
        .ok_or_else(|| JsValue::from_str(&format!("invalid f32 field: {key}")))?;
    // Field present but NaN/Infinity — this must propagate as an error,
    // not be silently coerced into the default (was the bug: `.unwrap_or(0.0)`
    // at the call site swallowed this Err exactly like the "missing" case).
    finite_f32(n, key).map(Some)
}

fn bool_from(obj: &JsValue, key: &str) -> Result<bool, JsValue> {
    let v = js_sys::Reflect::get(obj, &key.into())?;
    v.as_bool()
        .ok_or_else(|| JsValue::from_str(&format!("invalid bool field: {key}")))
}

fn optional_field(obj: &JsValue, key: &str) -> Result<Option<JsValue>, JsValue> {
    let v = js_sys::Reflect::get(obj, &key.into())?;
    if v.is_undefined() || v.is_null() {
        Ok(None)
    } else {
        Ok(Some(v))
    }
}

fn optional_string_field(obj: &JsValue, key: &str) -> Result<Option<String>, JsValue> {
    if let Some(v) = optional_field(obj, key)? {
        if let Some(s) = v.as_string() {
            Ok(Some(s))
        } else {
            Err(JsValue::from_str(&format!("invalid string field: {key}")))
        }
    } else {
        Ok(None)
    }
}
