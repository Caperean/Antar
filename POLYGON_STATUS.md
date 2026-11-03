# Polygon Rendering Status

## Status: rendering implemented, not yet verified on a GPU

`fillPolygon()` draws. The CPU path, the GPU upload path, and a fragment
shader that evaluates the fill are all wired end to end. What has **not** been
done is run it on a real GPU in a browser — see [Verification](#verification).

---

## What works

**CPU side (unchanged from before):**
- `PolygonDrawParams` with full JS parameter parsing
- `PolygonDrawParams::from_js`, `Color::from_js`
- `DrawCommand::FillPolygon` in the command queue
- Vertices transformed once on CPU in `resolve_fill_polygon`
- Public API: `fillPolygon()`, `setDebugMode()`, `setOutputGamma()`,
  `setMinPolygonAreaPx()`, `setEdgeSoftness()`
- Texture lookup validated eagerly at `fillPolygon()` time

**GPU side (new):**
- `shaders/polygon.wgsl` — vertex shader sizes a quad to the polygon's
  bounding box; fragment shader evaluates the even-odd signed distance
- Polygon render pipeline with its own bind group layout
- Storage-buffer bind group for metadata and vertices
- Textured fills batched by texture, like sprites
- 1×1 white dummy texture so untextured polygons always have a valid binding
- Polygons drawn inside the ordinary render pass, so they blend with sprites
  and rects through the existing premultiplied-alpha pipeline

---

## Why the compute rasterizer was abandoned

`shaders/rasterizer.wgsl` (tile-based, 16×16 workgroups) was written first,
never ran, and has been deleted. Three problems were structural:

1. **Barrier divergence.** It called `workgroupBarrier()` inside control flow
   where out-of-bounds invocations had already `return`ed and invalid polygons
   `continue`d past the barrier. Barriers must be in uniform control flow, so
   this is a spec violation, not a missing feature.

2. **Nothing composited its output.** It wrote to a
   `texture_storage_2d<rgba8unorm, write>` that was never sampled onto the
   surface, so even a working version would have produced a blank screen.

3. **It would have broken draw order.** A compute pass runs entirely before or
   after the render pass, so every polygon would have landed above or below
   every sprite regardless of the order commands were issued in.

The earlier claim that this was blocked on web-sys gaining `GpuComputePipeline`
was also incorrect: web-sys 0.3.102 already ships `GpuComputePipeline`,
`GpuComputePipelineDescriptor`, `GpuComputePassEncoder`, `begin_compute_pass`
and `dispatch_workgroups`. There was never anything to wait for.

## Why a fragment shader instead

The fragment shader reuses the same even-odd SDF maths, but evaluates it over a
single quad inside the existing render pass. That keeps premultiplied-alpha
blending, draw ordering, and the working sprite/rect pipeline untouched.

Known consequence: `FrameBatch` groups instances by type, so rects, sprites and
polygons each render as a block rather than in original command order. That
limitation predates this change.

---

## Bugs fixed along the way

- **`POLYGON_META_STRIDE` was 64; the data is 96 bytes.** Both the WGSL
  `PolygonMeta` struct and the Rust flatten are 24 × f32. The buffer was
  undersized and would have failed validation somewhere past ~409 polygons.
  It is now 96, and `flatten_polygon_meta` returns a fixed `[f32; 24]`, so a
  mismatch with `POLYGON_META_FLOATS` is a compile error.
- **`meta` is a reserved WGSL keyword.** The shader used `let meta = ...`,
  which fails at pipeline creation. Caught by `naga`, not by cargo.

---

## Verification

Run locally, all passing:

| Check | Result |
| --- | --- |
| `cargo check` | PASS, 0 warnings |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy -- -D warnings` | PASS |
| `cargo test --target x86_64-unknown-linux-gnu` | PASS, 13 tests |
| `wasm-pack build --release --target web` | PASS |
| `naga` on `common.wgsl` + each shader | PASS |

**Not verified:** rendering output in a browser. Nothing here confirms what the
shader actually draws — only that it compiles, type-checks, and that the data
reaching it matches the layout it expects. `tests/index.html` is the place to
check that visually.

---

## Still not done

- **Draw ordering.** Polygons, rects and sprites render as three blocks. True
  per-command ordering needs `FrameBatch` to keep a single ordered list.
- **SDF text.** The contour machinery here is the same shape glyph outlines
  would need, but no text path exists.
- **Performance tuning.** The fragment shader walks every edge for every pixel
  in the bounding box. Fine for typical 2D shapes; a large, many-sided polygon
  will cost proportionally more.

## API usage

```javascript
const ctx = await Antar.init(canvas);

ctx.fillPolygon({
  points: [100, 100, 200, 100, 150, 200],
  color: { r: 1, g: 0, b: 0, a: 1 },
  renderMode: "fill"
});

ctx.setDebugMode(0);        // 0=none, 1=SDF, 2=UV, 3=edges
ctx.setOutputGamma(2.2);
ctx.setMinPolygonAreaPx(1.0);
ctx.setEdgeSoftness(1.5);

ctx.present();
```

`renderMode` is `"fill"` (default, optional texture), `"outline"`
(`outlineWidth` in px), or `"mask"`.
