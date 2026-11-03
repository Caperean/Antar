# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

`0.1.0` is the first release and has not been tagged yet; everything below is
the initial feature set.

## [Unreleased]

## [0.1.0]

### Added

- **Polygon rendering.** `fillPolygon()` draws arbitrary vertex lists in
  `fill`, `outline` and `mask` modes, with an optional texture in fill mode.
  The even-odd signed-distance fill is evaluated per pixel in a fragment shader
  inside the normal render pass, so polygons blend with rectangles and sprites.
  Consecutive polygons sharing a texture are batched into one draw call.
- Polygon tuning: `setEdgeSoftness`, `setDebugMode`, `setOutputGamma`,
  `setMinPolygonAreaPx`.
- 13 unit tests covering the projection matrix, buffer strides, the polygon
  metadata layout, and polygon parameter resolution — the first tests in the
  crate.
- CI: clippy (`-D warnings`), rustfmt, tests on the host triple, and a release
  wasm build.

### Fixed

- **Canvas resize was ignored.** The projection matrix was built from the
  dimensions captured at `init`, so any window resize or DPR change stretched
  and misaligned every subsequent draw. Dimensions are now re-read each frame,
  and zero-size canvases are skipped instead of producing a surface error.
- **`POLYGON_META_STRIDE` was 64 bytes; the data is 96.** The storage buffer was
  undersized and would have failed validation somewhere past ~409 polygons. The
  metadata flatten now returns a fixed-size array so a layout mismatch is a
  compile error rather than a buffer overrun.
- **GPU validation errors were invisible.** No `uncapturederror` handler was
  registered, so pipeline and bind-group failures surfaced only as a blank
  canvas. Errors and device loss are now reported to the console.
- **Tags published without running CI.** `ci.yml` triggers on branch pushes
  only, so a tag push could release code that never compiled. The release
  workflow now runs format, clippy and tests before building.
- **Release workflow:** `action-gh-release` bumped to v2, and the wasm-pack
  binary is cached instead of compiled on every release.

### Removed

- `src/gpu/shaders/rasterizer.wgsl` and its unused compute scaffolding. The
  shader could not run as written — it called `workgroupBarrier()` inside
  control flow that out-of-bounds invocations had already returned from, it
  wrote to a storage texture nothing composited, and a separate compute pass
  would have broken draw ordering.

### Known limitations

- Draw order is grouped by instance type (rectangles, then sprites, then
  polygons) rather than by call order.
- No text rendering.
- Device loss is reported but not recovered.
- Polygon output has not been verified on a GPU in a browser.
