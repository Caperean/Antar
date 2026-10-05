use super::types::Rect;

/// A single packed sprite inside the atlas.
#[derive(Clone, Copy, Debug)]
pub struct AtlasEntry {
    /// UV coordinates in [0.0, 1.0] space.
    pub uv: Rect,
    /// Pixel coordinates inside the atlas texture.
    pub pixel: Rect,
    /// The atlas-texture handle (set after GpuContext::load_texture).
    pub atlas_id: u32,
}

/// Describes one image to be packed into the atlas.
pub struct AtlasSource {
    pub key: String,
    pub width: u32,
    pub height: u32,
    /// Raw RGBA8 pixel data.
    pub data: Vec<u8>,
}

/// Shelf-based rectangle packer.
///
/// This is a simple O(n) online packer: it fills horizontal "shelves"
/// top-to-bottom.  Good enough for sprite sheets; swap in a more
/// sophisticated algorithm (e.g. guillotine, MaxRects) if needed.
struct ShelfPacker {
    atlas_w: u32,
    atlas_h: u32,
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
}

impl ShelfPacker {
    fn new(atlas_w: u32, atlas_h: u32) -> Self {
        Self { atlas_w, atlas_h, shelf_x: 0, shelf_y: 0, shelf_h: 0 }
    }

    /// Try to place a rectangle of size (w, h).  Returns the top-left
    /// pixel position if successful.
    fn pack(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w > self.atlas_w || h > self.atlas_h {
            return None;
        }

        // Advance to next shelf if no room horizontally.
        if self.shelf_x + w > self.atlas_w {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }

        if self.shelf_y + h > self.atlas_h {
            return None; // Atlas is full.
        }

        let pos = (self.shelf_x, self.shelf_y);
        self.shelf_x += w;
        self.shelf_h = self.shelf_h.max(h);
        Some(pos)
    }
}

/// An in-progress atlas build result.
pub struct TextureAtlas {
    /// RGBA8 pixel buffer for the entire atlas.
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Mapping from sprite key → packed UV / pixel info.
    pub entries: std::collections::HashMap<String, AtlasEntry>,
}

/// Build a texture atlas from a list of sources.
///
/// `atlas_size` is the side length of the square atlas texture (e.g. 2048).
/// Returns `None` if any sprite cannot be packed (atlas too small).
pub fn create_texture_atlas(
    sources: Vec<AtlasSource>,
    atlas_size: u32,
) -> Option<TextureAtlas> {
    let mut packer = ShelfPacker::new(atlas_size, atlas_size);
    let mut pixels = vec![0u8; (atlas_size * atlas_size * 4) as usize];
    let mut entries = std::collections::HashMap::new();

    for src in sources {
        let (px, py) = packer.pack(src.width, src.height)?;

        for row in 0..src.height {
            let src_start = (row * src.width * 4) as usize;
            let dst_start = ((py + row) * atlas_size * 4 + px * 4) as usize;
            let len = (src.width * 4) as usize;
            pixels[dst_start..dst_start + len]
                .copy_from_slice(&src.data[src_start..src_start + len]);
        }

        let aw = atlas_size as f32;
        let ah = atlas_size as f32;
        entries.insert(
            src.key,
            AtlasEntry {
                pixel: Rect::new(px as f32, py as f32, src.width as f32, src.height as f32),
                uv: Rect::new(
                    px as f32 / aw,
                    py as f32 / ah,
                    src.width as f32 / aw,
                    src.height as f32 / ah,
                ),
                atlas_id: 0,
            },
        );
    }

    Some(TextureAtlas {
        pixels,
        width: atlas_size,
        height: atlas_size,
        entries,
    })
}

impl TextureAtlas {
    /// Assign the GPU texture handle after upload.
    pub fn set_atlas_id(&mut self, atlas_id: u32) {
        for entry in self.entries.values_mut() {
            entry.atlas_id = atlas_id;
        }
    }
}
