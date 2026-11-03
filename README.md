# Antar

A 2D WebGPU renderer for the browser, written in Rust and compiled to WebAssembly.

Antar batches 2D draw commands on the CPU and submits one frame at a time to a
WebGPU device: rectangles, sprites, and arbitrary polygons. It is small, has no
runtime dependencies beyond the browser's WebGPU implementation, and nothing is
drawn until you call `present()`.

## Status

`0.1.0` — early. The API works and is covered by unit tests, but see
[Limitations](#limitations) before depending on it. Polygon output has not yet
been verified on a GPU in a browser; the shader compiles and is validated, but
no one has compared its pixels against a reference.

## Features

- **Rectangles and sprites** — instanced quads, premultiplied-alpha blending
- **Polygons** — arbitrary vertex lists, `fill` / `outline` / `mask` modes,
  optional texture, per-pixel even-odd fill with anti-aliased edges
- **Transforms** — save/restore stack with translate, rotate, scale
- **Textures** — RGBA upload and atlas creation
- **Resize-aware** — the projection matrix is rebuilt when the canvas changes
- **GPU error reporting** — validation failures and device loss are surfaced to
  the console instead of being silently swallowed

## Building

Requires a Rust toolchain and, for the wasm target:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

wasm-pack build --release --target web
```

Output lands in `pkg/`: `antar.js`, `antar_bg.wasm`, and TypeScript types.
The crate is not published to npm yet; consume `pkg/` directly or install it
from the repository.

## Quick start

```html
<canvas id="canvas" width="800" height="600"></canvas>
<script type="module">
  import init, { Antar } from "./pkg/antar.js";

  await init();
  const ctx = await Antar.init(document.getElementById("canvas"));

  ctx.clear("#1a1a2e");

  ctx.fillRect({
    rect: { x: 100, y: 100, w: 200, h: 120 },
    color: { r: 1, g: 0.2, b: 0.2, a: 1 },
  });

  ctx.fillPolygon({
    points: [400, 100, 550, 100, 475, 240], // flat x,y pairs, 3+ vertices
    color: { r: 0.2, g: 0.9, b: 0.6, a: 1 },
  });

  ctx.present();
</script>
```

`present()` is what uploads and draws. Calls before it only enqueue commands.

## API

| Method | Purpose |
| --- | --- |
| `Antar.init(canvas)` | Create the device, surface and pipelines |
| `clear(color)` | Set the frame clear colour (`#rrggbb` / `#rrggbbaa`) |
| `fillRect(params)` | Filled rectangle |
| `drawImage(params)` | Textured quad / sprite |
| `fillPolygon(params)` | Arbitrary polygon, `fill` / `outline` / `mask` |
| `present()` | Resolve the queue and submit the frame |
| `setFillStyle(css)` | Default fill colour |
| `setGlobalAlpha(a)` | Global alpha multiplier |
| `translate` / `rotate` / `scale` | Transform the current state |
| `save()` / `restore()` | Push / pop transform state |
| `loadTexture(data, w, h)` | Upload RGBA pixels, returns a handle |
| `unloadTexture(id)` | Release a texture |
| `createTextureAtlas(...)` | Build a packed atlas |
| `setEdgeSoftness(px)` | Polygon edge anti-aliasing width |
| `setDebugMode(mode)` | `0` none, `1` SDF, `2` UV, `3` edges |
| `setOutputGamma(g)` | Gamma applied to polygon output |
| `setMinPolygonAreaPx(a)` | Skip polygons smaller than this |

Full parameter tables and error behaviour: [docs/API.md](docs/API.md).
Architecture and build notes: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Browser support

Any browser with WebGPU enabled — Chrome/Edge 113+, and Safari/Firefox behind
their respective flags. There is no WebGL fallback; if `navigator.gpu` is
absent, `Antar.init()` throws.

## Limitations

- **Draw order is grouped by type, not by call order.** Rectangles, then
  sprites, then polygons are each drawn as a block regardless of the order you
  issued them in, so a sprite can be occluded by a polygon drawn earlier. This
  is the most significant known correctness issue.
- **No text rendering.** Glyph contours would reuse the polygon machinery, but
  no text path exists yet.
- **Device loss is reported, not recovered.** Losing the GPU device (tab
  backgrounding, driver reset) logs a message but does not re-initialise.
- **Polygon cost scales with edge count per pixel.** Fine for typical shapes;
  a large many-sided polygon covering the screen is proportionally expensive.
- **Only the `web` wasm-pack target is built.** No `bundler` target yet.

## License

MIT — see [LICENSE](LICENSE).
