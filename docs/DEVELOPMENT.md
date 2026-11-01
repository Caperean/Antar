# Antar Development Guide

How the codebase is organized and how to extend it safely.

---

## Architecture overview

```
JS / TS
  └─ Antar (src/antar.rs)          # wasm-bindgen surface
       ├─ Renderer2D (CPU)         # commands → FrameBatch
       └─ GpuContext (WebGPU)      # textures, pipelines, draw calls
```

**Hard rule:** `renderer/` must not import `web_sys` GPU types. Only `gpu/` may touch WebGPU (`GpuDevice`, `GpuTexture`, …). The CPU side only knows `TextureHandle(u32)`.

Frame flow:

1. JS calls API methods on `Antar`  
2. `Antar` pushes `DrawCommand` into `Renderer2D`  
3. `present()` → `flush_commands()` resolves state into `SpriteInstance` / `RectInstance` (`FrameBatch`)  
4. `GpuContext::execute(&FrameBatch)` converts to float buffers, batches by texture, draws  

```
DrawCommand queue  →  FrameBatch  →  GPU instance buffers  →  render pass
```

---

## Repository layout

```
src/
├── antar.rs              # Public WASM API
├── lib.rs                # crate root (mod renderer; mod gpu; mod antar)
├── antar-shim.js         # Thin JS helper (optional)
│
├── renderer/             # CPU only — no WebGPU
│   ├── renderer2d.rs     # Renderer2D
│   ├── command.rs        # DrawCommand, FrameBatch, CommandQueue
│   ├── params.rs         # JsValue → ImageDrawParams / RectDrawParams / PolygonDrawParams
│   ├── state.rs          # save/restore stack
│   ├── atlas.rs          # CPU shelf packer
│   └── types.rs          # Rect, Color, Transform, instances, TextureHandle, PolygonInstance
│
└── gpu/                  # WebGPU backend
    ├── mod.rs            # GpuContext
    ├── texture.rs        # TextureRegistry
    ├── device.rs / surface.rs / pipeline.rs / buffers.rs
    ├── bindgroups.rs / encoder.rs
    └── shaders/          # common.wgsl, sprite.wgsl, rect.wgsl, rasterizer.wgsl

docs/
├── API.md                # Public renderer commands (this is for consumers)
└── DEVELOPMENT.md        # This file

tests/                    # Browser smoke test (fillRect square)
pkg/                      # wasm-pack output
```

Design notes also live in `zalozenia.txt` (English version now). Prefer this English guide for day-to-day development.

---

## Toolchain

### Prerequisites

- Rust (`wasm32-unknown-unknown` target)
- `wasm-pack`
- Chrome / Edge with WebGPU for manual tests

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

`.cargo/config.toml` sets:

- default target `wasm32-unknown-unknown`
- `--cfg=web_sys_unstable_apis` (required for WebGPU bindings)

### Build

```bash
# Check
cargo check

# Browser package → pkg/
wasm-pack build --release --target web
```

`wasm-opt` is disabled in `Cargo.toml` metadata (binaryen download issues on some Windows setups). Re-enable later if you install Binaryen locally:

```toml
[package.metadata.wasm-pack.profile.release]
wasm-opt = false
```

### Copy into the test page

```bash
# PowerShell
Copy-Item -Force pkg/antar.js,pkg/antar_bg.wasm,pkg/antar.d.ts tests/
cd tests
python -m http.server 8080
# open http://localhost:8080/
```

---

## Core types (CPU)

| Type | Role |
|------|------|
| `DrawCommand` | High-level queued ops from JS |
| `FrameBatch` | Per-frame clear + sprites + rects + polygons after resolve |
| `SpriteInstance` | Baked sprite (texture id, src/dst, transform, color, alpha) |
| `RectInstance` | Baked solid rect |
| `PolygonInstance` | Baked polygon (metadata + transformed vertices) |
| `TextureHandle` | Opaque `u32` ID; GPU texture lives in `TextureRegistry` |
| `Transform` | 2D affine (`a,b,c,d,tx,ty`) |

`SpriteInstance.src` is in **texture pixels**. `GpuContext` converts to normalized UVs using registered texture sizes.

---

## Adding a new draw command

Example: `fillCircle` (or see `fillPolygon` for a compute shader example).

1. **Params** (`renderer/params.rs`) — struct + `from_js`
2. **Command** (`renderer/command.rs`) — add `DrawCommand::FillCircle(...)`
3. **Resolve** (`renderer/renderer2d.rs`) — push a new instance type or reuse rects
4. **GPU** (if new primitive) — WGSL + pipeline + instance layout in `gpu/`
5. **API** (`antar.rs`) — `#[wasm_bindgen(js_name = fillCircle)]` returning `Result<(), JsValue>`
6. **Docs** — update `docs/API.md`

Keep validation in `from_js` / API (finite floats, lengths). Prefer `Result` over `.expect()` / panics — WASM panics kill the tab.

---

## Adding GPU features

| Goal | Where |
|------|-------|
| New shader | `gpu/shaders/*.wgsl` + concatenate with `common.wgsl` in `pipeline.rs` |
| New render pipeline | `gpu/pipeline.rs` |
| New compute pipeline | `gpu/pipeline.rs` (see `build_rasterizer_pipeline`) |
| New buffer layout | Keep stride in sync with `gpu/mod.rs` float packing **and** vertex attributes |
| Texture storage | Only `gpu/texture.rs` / `GpuContext::load_texture` |
| Storage buffers | `gpu/buffers.rs::create_storage_buffer` |

**Gotcha:** the shared unit-quad vertex buffer is stride **16** (`pos` + `uv`). Pipelines that only use `pos` must still set array stride `16`, not `8`.

Instance packing must match shader locations exactly (sprite: 18 floats, rect: 14 floats, polygon: 16 floats per metadata).

---

## Atlas packing

CPU packer: `renderer/atlas.rs` (shelf algorithm).

1. Pack RGBA into one bitmap  
2. `GpuContext::load_texture` uploads it  
3. Call `TextureAtlas::set_atlas_id(handle)` after upload  

JS gets `{ atlasId, uvs }`. Draw with `drawImage({ texture: atlasId, src: pixelRect, dst: ... })` — convert UV back to pixels if you only stored normalized UVs, or keep pixel rects from packing if you expose them later.

---

## Performance rules

- Reuse heap buffers across frames (`CommandQueue`, `FrameBatch`, GPU scratch `Vec`s) — `clear()` / `clear_retain()`, do not allocate a new `Vec` every `present()`.
- Batch sprites by texture ID in `GpuContext::execute`.
- Prefer one atlas texture over many small textures when possible.
- Never allocate or parse CSS in the hot path beyond what’s needed; keep JS objects small.

---

## Testing

| Test | Path | Purpose |
|------|------|---------|
| `fillRect` square | `tests/index.html` | Visual smoke test |

Checklist for a new feature:

1. `cargo check`  
2. `wasm-pack build --release --target web`  
3. Copy artifacts into `tests/`  
4. Hard-refresh the browser page  
5. Confirm no console / GPU errors  

---

## Conventions

- Public JS names: camelCase via `#[wasm_bindgen(js_name = ...)]`  
- Rust modules: snake_case  
- Fail closed on bad JS input (`Result` + clear error strings)  
- Reject non-finite floats before they enter `Transform::concat`  
- Recycle texture IDs on unload; never panic on `u32` overflow — return `JsValue` error  

---

## Future extension points

Good candidates that fit the current split:

- 2D camera (extra transform at resolve or uniform)  
- Layers / z-order (sort instances before upload)  
- Tilemaps (atlas + batched sprites)  
- Bitmap / SDF text  
- Render-to-texture (extra surface in `gpu/`)  

When adding a backend (e.g. WebGL), keep `Renderer2D` + `FrameBatch` unchanged and swap only `GpuContext`.
