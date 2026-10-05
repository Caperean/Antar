# Antar Renderer API

Public JavaScript / TypeScript API for the Antar 2D WebGPU renderer.

Nothing is drawn immediately. Calls enqueue commands; `present()` resolves state on the CPU and submits a batched frame to the GPU.

---

## Setup

```js
import init, { Antar } from "../pkg/antar.js";

await init();
const ctx = await Antar.init(document.getElementById("canvas"));
```

Requires a browser with WebGPU (`navigator.gpu`).

Minimal frame:

```js
ctx.clear("#1a1a2e");
ctx.fillRect({
  rect: { x: 100, y: 100, w: 200, h: 200 },
  color: { r: 1, g: 0.2, b: 0.2, a: 1 },
});
ctx.present();
```

---

## Lifecycle

### `Antar.init(canvas)` → `Promise<Antar>`

Creates GPU device, surface, pipelines, and a CPU-side `Renderer2D`.

| Argument | Type | Description |
|----------|------|-------------|
| `canvas` | `HTMLCanvasElement` | Canvas used as the WebGPU swapchain |

Throws if WebGPU init fails.

### `clear(color)`

Queues a full-screen clear for the next `present()`.

| Argument | Type | Description |
|----------|------|-------------|
| `color` | `string` | CSS hex `#rrggbb` or `#rrggbbaa`. Invalid → black |

Clear runs as `GPULoadOp.Clear` (no full-screen draw call).

### `present()` → `void` (throws on error)

1. Flushes the command queue  
2. Resolves transforms / fill style / alpha into `SpriteInstance` / `RectInstance`  
3. Uploads instance buffers and draws on the GPU  

Must be called once per frame after draw commands.

---

## Drawing

### `drawImage(params)` → `void` (throws on invalid params)

Draws a textured quad (sprite / atlas region).

```js
ctx.drawImage({
  texture: texId,                    // required — handle from loadTexture / atlas
  src: { x: 0, y: 0, w: 32, h: 32 }, // required — source rect in texture pixels
  dst: { x: 100, y: 200, w: 64, h: 64 }, // required — destination on screen
  rotation: 0,                       // optional radians (around dst center)
  color: { r: 1, g: 1, b: 1, a: 1 }, // optional tint (default white)
  flipX: false,                      // optional
  flipY: false,                      // optional
});
```

| Field | Required | Notes |
|-------|----------|-------|
| `texture` | yes | `u32` texture handle |
| `src` | yes | Pixel rect in the texture |
| `dst` | yes | Screen-space rect |
| `rotation` | no | Finite number; default `0` |
| `color` | no | RGBA in `[0, 1]`; default white |
| `flipX` / `flipY` | no | Booleans; default `false` |

NaN / Infinity in numeric fields throw a JS error (no silent NaN transforms).

### `fillRect(params)` → `void` (throws on invalid params)

Fills an axis-aligned rectangle.

```js
ctx.fillRect({
  rect: { x: 0, y: 0, w: 50, h: 50 },
  color: { r: 1, g: 0, b: 0, a: 1 }, // optional
});
```

| Field | Required | Notes |
|-------|----------|-------|
| `rect` | yes | `{ x, y, w, h }` in pixels |
| `color` | no | If omitted or white, uses current `setFillStyle` |

### `fillPolygon(params)` → `void` (throws on invalid params)

Fills an arbitrary polygon using SDF-based rasterization (compute shader).

```js
ctx.fillPolygon({
  points: [0, 0, 100, 0, 50, 100], // flat list: x0, y0, x1, y1, ... (min 3 vertices)
  color: { r: 1, g: 0, b: 0, a: 1 }, // optional, default white
  renderMode: "fill", // optional: "fill" | "outline" | "mask", default "fill"
  outlineWidth: 4, // optional, px, only for OUTLINE mode
  texture: texId, // optional, only for FILL mode
  skipIfTransparent: false, // optional performance hint
});
```

|| Field | Required | Notes |
||-------|----------|-------|
|| `points` | yes | Flat array of `[x, y, x, y, ...]` — at least 3 vertices (6 numbers) |
|| `color` | no | RGBA in `[0, 1]`; default white |
|| `renderMode` | no | `"fill"`, `"outline"`, or `"mask"`; default `"fill"` |
|| `outlineWidth` | no | Outline thickness in pixels; only used in OUTLINE mode |
|| `texture` | no | Texture handle for FILL mode; optional |
|| `skipIfTransparent` | no | Performance hint; default `false` |

**Note:** Polygon rendering infrastructure is production-ready on the CPU side. GPU execution via compute shader requires web-sys with `GpuComputePipeline` support (pending web-sys update). Currently, polygons are queued, processed, and data is uploaded to storage buffers, but actual rasterization is pending compute shader integration.

---

## Drawing state

### `setFillStyle(css)`

Sets the fill color for later `fillRect` calls that do not pass a custom color.

| Argument | Type | Description |
|----------|------|-------------|
| `css` | `string` | `#rrggbb` or `#rrggbbaa`. Invalid → white |

### `setGlobalAlpha(alpha)` → `void` (throws if not finite)

| Argument | Type | Range |
|----------|------|-------|
| `alpha` | `number` | Clamped to `[0, 1]` |

Multiplies into sprite tint alpha and rect fill alpha at resolve time.

---

## Transforms

Canvas-style, applied in call order. Baked into each instance at `present()`.

### `translate(x, y)`

### `rotate(angle)`

`angle` in **radians**.

### `scale(sx, sy)`

### `save()` / `restore()`

Push / pop transform, fill style, and global alpha.

All transform arguments must be finite numbers or the call throws.

```js
ctx.save();
ctx.translate(400, 300);
ctx.rotate(Math.PI / 4);
ctx.fillRect({
  rect: { x: -50, y: -50, w: 100, h: 100 },
  color: { r: 0.2, g: 0.8, b: 1, a: 1 },
});
ctx.restore();
```

---

## Textures

### `loadTexture(rgbaData, width, height)` → `number`

Uploads raw **RGBA8** pixels and returns a texture handle (`u32`).

| Argument | Type | Description |
|----------|------|-------------|
| `rgbaData` | `Uint8Array` | Length must be `width * height * 4` |
| `width` | `number` | Texture width in pixels |
| `height` | `number` | Texture height in pixels |

```js
const id = ctx.loadTexture(imageData.data, imageData.width, imageData.height);
```

### `unloadTexture(textureId)`

Destroys the GPU texture and recycles the handle.

### `createTextureAtlas(keys, widths, heights, dataFlat, atlasSize)` → `object`

Packs multiple RGBA sprites on the CPU, uploads one atlas texture.

| Argument | Type | Description |
|----------|------|-------------|
| `keys` | `string[]` | Sprite names |
| `widths` | `Uint32Array` / `number[]` | Per-sprite widths |
| `heights` | `Uint32Array` / `number[]` | Per-sprite heights |
| `dataFlat` | `Uint8Array` | Concatenated RGBA blobs in the same order as `keys` |
| `atlasSize` | `number` | Side length of the square atlas (e.g. `2048`) |

Returns:

```js
{
  atlasId: 1,           // texture handle for drawImage
  uvs: {
    "hero": [u, v, w, h], // normalized UV rects in [0, 1]
    // ...
  }
}
```

Throws if array lengths mismatch, `dataFlat` is too short/long, or packing fails (atlas too small).

---

## Polygon rendering configuration

These methods control global polygon rendering behavior (bypass command queue, write directly to GPU context).

### `setDebugMode(mode)`

Sets debug visualization mode for polygon rendering.

|| Argument | Type | Description |
||----------|------|-------------|
|| `mode` | `number` | `0` = none, `1` = SDF visualization, `2` = UV debug, `3` = edge debug |

### `setOutputGamma(gamma)`

Sets gamma correction for polygon output (applied only when debug mode is none).

|| Argument | Type | Description |
||----------|------|-------------|
|| `gamma` | `number` | Gamma value (e.g., `2.2` for sRGB, `1.0` for no correction) |

### `setMinPolygonAreaPx(area)`

Sets minimum screen-space area threshold for polygon culling.

|| Argument | Type | Description |
||----------|------|-------------|
|| `area` | `number` | Minimum area in pixels²; polygons below this are skipped |

### `setEdgeSoftness(softness)`

Controls anti-aliasing edge softness for polygon SDF rendering.

|| Argument | Type | Description |
||----------|------|-------------|
|| `softness` | `number` | Edge softness in pixels; higher = smoother edges |

---

## Shared value types

### Rect `{ x, y, w, h }`

All fields: finite `number` (pixels unless noted as UV).

### Color `{ r, g, b, a? }`

Components in `[0, 1]`. `a` defaults to `1` if omitted.

### Hex colors

Accepted by `clear` and `setFillStyle`: `#rrggbb`, `#rrggbbaa`.

---

## Internal command mapping

| JS call | Queued `DrawCommand` |
|---------|----------------------|
| `clear` | `Clear(Color)` |
| `drawImage` | `DrawImage(ImageDrawParams)` |
| `fillRect` | `FillRect(RectDrawParams)` |
| `fillPolygon` | `FillPolygon(PolygonDrawParams)` |
| `setFillStyle` | `SetFillStyle(Color)` |
| `setGlobalAlpha` | `SetAlpha(f32)` |
| `translate` / `rotate` / `scale` | `Transform(...)` |
| `save` / `restore` | `PushState` / `PopState` |

At `present()`, these become a `FrameBatch` (`clear` + `SpriteInstance[]` + `RectInstance[]` + `PolygonInstance[]`) executed by `GpuContext`.

---

## Error handling

Methods that return `Result` in Rust surface as thrown JS exceptions (or rejected promises for `init`). Always wrap production draw code:

```js
try {
  ctx.drawImage({ /* ... */ });
  ctx.present();
} catch (e) {
  console.error(e);
}
```

Common failures: missing object fields, NaN/Infinity, texture size mismatch, atlas packing failure, WebGPU device loss.
