use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use web_sys::GpuTexture;

/// A loaded GPU texture with its original pixel dimensions.
pub struct TextureEntry {
    pub texture: GpuTexture,
    pub width: u32,
    pub height: u32,
}

/// Handle → GPU texture registry. Lives only in `GpuContext`.
///
/// Texture IDs are monotonically increasing u32 handles, with freed IDs
/// recycled on `unload`. ID 0 is reserved for "no texture" / invalid.
pub struct TextureRegistry {
    next_id: u32,
    free_ids: Vec<u32>,
    textures: HashMap<u32, TextureEntry>,
}

impl TextureRegistry {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            free_ids: Vec::new(),
            textures: HashMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        texture: GpuTexture,
        width: u32,
        height: u32,
    ) -> Result<u32, JsValue> {
        let id = if let Some(id) = self.free_ids.pop() {
            id
        } else {
            let id = self.next_id;
            self.next_id = self
                .next_id
                .checked_add(1)
                .ok_or_else(|| JsValue::from_str("texture handle limit reached"))?;
            id
        };
        self.textures
            .insert(id, TextureEntry { texture, width, height });
        Ok(id)
    }

    pub fn get(&self, id: u32) -> Option<&TextureEntry> {
        self.textures.get(&id)
    }

    pub fn has(&self, id: u32) -> bool {
        self.textures.contains_key(&id)
    }

    pub fn unload(&mut self, id: u32) {
        if id == 0 {
            return;
        }
        if let Some(entry) = self.textures.remove(&id) {
            entry.texture.destroy();
            self.free_ids.push(id);
        }
    }

    pub fn clear(&mut self) {
        for (_, entry) in self.textures.drain() {
            entry.texture.destroy();
        }
        self.free_ids.clear();
        self.next_id = 1;
    }
}
