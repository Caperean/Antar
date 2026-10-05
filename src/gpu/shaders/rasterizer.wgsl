// ============================================================================
// Ultra-fast 2D rasterizer — TBR (16x16) + SDF + transforms + blending — v3
// Single compute pass, no fragment shader, no triangulation.
// ============================================================================
//
// v3 additions vs. the multi-polygon v2 shader:
//  - per-polygon render_mode: FILL / OUTLINE / MASK
//  - global debug_mode: NONE / SDF / UV / EDGES (overrides normal shading,
//    blending across polygons still applies)
//  - gamma correction on final output (skipped in debug modes)
//  - per-polygon FLAG_SKIP_TRANSPARENT: host-precomputed "this polygon's
//    texture region is empty, don't even evaluate it" fast-skip
//  - global min_polygon_area_px: cull polygons whose approximate
//    screen-space bounding box is smaller than a pixel threshold, before
//    any vertex work is done
//
// ============================================================================

const TILE_SIZE: u32 = 16u;
const MAX_VERTICES: u32 = 256u;

const MODE_FILL: u32 = 0u;
const MODE_OUTLINE: u32 = 1u;
const MODE_MASK: u32 = 2u;

const DEBUG_NONE: u32 = 0u;
const DEBUG_SDF: u32 = 1u;
const DEBUG_UV: u32 = 2u;
const DEBUG_EDGES: u32 = 3u;

const FLAG_SKIP_TRANSPARENT: u32 = 1u;

struct Uniforms {
    screen_size          : vec2<f32>,
    polygon_count        : u32,
    edge_softness         : f32,
    debug_mode             : u32,  // DEBUG_* — overrides render_mode for the whole dispatch
    output_gamma            : f32, // e.g. 2.2; 1.0 = no correction. Applied only when debug_mode == DEBUG_NONE
    min_polygon_area_px      : f32, // approx screen-space bbox area (px^2) below which a polygon is culled
    _pad0                     : f32,
};

struct PolygonMeta {
    vertex_offset : u32,        // index into `polygonVertices`
    vertex_count  : u32,
    translate     : vec2<f32>,
    scale         : vec2<f32>,  // must be non-zero on both axes
    rotation      : f32,        // radians
    render_mode   : u32,        // MODE_FILL / MODE_OUTLINE / MODE_MASK
    bbox_min      : vec2<f32>,  // LOCAL-space (pre-transform) AABB
    bbox_max      : vec2<f32>,
    outline_width : f32,        // used when render_mode == MODE_OUTLINE, in screen px
    flags         : u32,        // bitmask, see FLAG_*
    _pad1         : vec2<f32>,  // alignment padding before tint_color (vec4 needs 16B alignment)
    tint_color    : vec4<f32>,  // OUTLINE/MASK: solid color. FILL: multiplied with sampled texel.
};

@group(0) @binding(0) var<uniform> u : Uniforms;
@group(0) @binding(1) var<storage, read> polygons : array<PolygonMeta>;
@group(0) @binding(2) var<storage, read> polygonVertices : array<vec2<f32>>;
@group(0) @binding(3) var srcTexture : texture_2d<f32>;
@group(0) @binding(4) var srcSampler : sampler;
@group(0) @binding(5) var outTexture : texture_storage_2d<rgba8unorm, write>;

var<workgroup> wgVerts : array<vec2<f32>, MAX_VERTICES>;
var<workgroup> wgCount : u32;

// ----------------------------------------------------------------------------
fn sdSegment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    return length(pa - ba * h);
}

// Signed distance to the polygon currently held in workgroup memory.
// Arbitrary (incl. non-convex) polygons; sign via even-odd crossing number.
fn sdPolygonLocal(p: vec2<f32>, count: u32) -> f32 {
    var d: f32 = 1e10;
    var inside: bool = false;

    var j: u32 = count - 1u;
    for (var i: u32 = 0u; i < count; i = i + 1u) {
        let vi = wgVerts[i];
        let vj = wgVerts[j];

        d = min(d, sdSegment(p, vi, vj));

        let cond = ((vi.y > p.y) != (vj.y > p.y)) &&
                   (p.x < (vj.x - vi.x) * (p.y - vi.y) / (vj.y - vi.y) + vi.x);
        if (cond) {
            inside = !inside;
        }

        j = i;
    }

    return select(d, -d, inside);
}

// ----------------------------------------------------------------------------
// Transform helpers
// ----------------------------------------------------------------------------
fn rotate2(v: vec2<f32>, angle: f32) -> vec2<f32> {
    let s = sin(angle);
    let c = cos(angle);
    return vec2<f32>(v.x * c - v.y * s, v.x * s + v.y * c);
}

fn applyTransform(localP: vec2<f32>, meta: PolygonMeta) -> vec2<f32> {
    let scaled = localP * meta.scale;
    let rotated = rotate2(scaled, meta.rotation);
    return rotated + meta.translate;
}

fn inverseTransform(screenP: vec2<f32>, meta: PolygonMeta) -> vec2<f32> {
    let translated = screenP - meta.translate;
    let unrotated = rotate2(translated, -meta.rotation);
    return unrotated / meta.scale;
}

fn computeUV(screenP: vec2<f32>, meta: PolygonMeta) -> vec2<f32> {
    let localP = inverseTransform(screenP, meta);
    let bboxSize = max(meta.bbox_max - meta.bbox_min, vec2<f32>(1e-4, 1e-4));
    return (localP - meta.bbox_min) / bboxSize;
}

fn uvInBounds(uv: vec2<f32>) -> bool {
    return uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0;
}

// ----------------------------------------------------------------------------
// Validation — malformed polygons are skipped entirely.
// ----------------------------------------------------------------------------
fn isValidPolygon(meta: PolygonMeta) -> bool {
    if (meta.vertex_count < 3u) {
        return false;
    }
    if (meta.vertex_count > MAX_VERTICES) {
        return false;
    }
    if (meta.scale.x == 0.0 || meta.scale.y == 0.0) {
        return false;
    }
    let size = meta.bbox_max - meta.bbox_min;
    if (size.x <= 1e-6 || size.y <= 1e-6) {
        return false;
    }
    return true;
}

// ----------------------------------------------------------------------------
// SDF debug palette: sign-colored, banded every 10px, boundary highlighted.
// ----------------------------------------------------------------------------
fn sdfDebugColor(d: f32) -> vec3<f32> {
    let insideColor = vec3<f32>(0.25, 0.45, 1.0);
    let outsideColor = vec3<f32>(1.0, 0.55, 0.15);
    var base = select(outsideColor, insideColor, d < 0.0);

    let stripe = abs(fract(d / 10.0) - 0.5) * 2.0;
    base = base * mix(0.35, 1.0, stripe);

    let edgeLine = 1.0 - smoothstep(0.0, 1.5, abs(d));
    base = mix(base, vec3<f32>(1.0, 1.0, 1.0), edgeLine);

    return base;
}

// ----------------------------------------------------------------------------
// Main pass: one invocation = one pixel, one workgroup = one 16x16 tile.
// ----------------------------------------------------------------------------
@compute @workgroup_size(TILE_SIZE, TILE_SIZE, 1)
fn main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) lidx: u32
) {
    let inBounds = f32(gid.x) < u.screen_size.x && f32(gid.y) < u.screen_size.y;
    let p = vec2<f32>(f32(gid.x) + 0.5, f32(gid.y) + 0.5);

    var accumColor: vec3<f32> = vec3<f32>(0.0, 0.0, 0.0);
    var accumAlpha: f32 = 0.0;

    let threadsPerTile = TILE_SIZE * TILE_SIZE;

    for (var pi: u32 = 0u; pi < u.polygon_count; pi = pi + 1u) {
        let meta = polygons[pi];

        if (!isValidPolygon(meta)) {
            continue;
        }
        if ((meta.flags & FLAG_SKIP_TRANSPARENT) != 0u) {
            continue;
        }

        // Cheap screen-space size estimate (ignores rotation's effect on
        // the AABB — conservative enough for a culling heuristic, not
        // meant to be exact).
        let approxSize = (meta.bbox_max - meta.bbox_min) * abs(meta.scale);
        if (approxSize.x * approxSize.y < u.min_polygon_area_px) {
            continue;
        }

        let count = meta.vertex_count;

        // --- cooperative load + transform into workgroup memory ---
        var i: u32 = lidx;
        loop {
            if (i >= count) { break; }
            let localV = polygonVertices[meta.vertex_offset + i];
            wgVerts[i] = applyTransform(localV, meta);
            i = i + threadsPerTile;
        }
        if (lidx == 0u) {
            wgCount = count;
        }
        workgroupBarrier();

        if (inBounds) {
            let d = sdPolygonLocal(p, wgCount);

            var srcColor: vec3<f32>; // zero-initialized by WGSL if unassigned
            var srcAlpha: f32 = 0.0;

            if (u.debug_mode == DEBUG_SDF) {
                srcColor = sdfDebugColor(d);
                srcAlpha = 1.0;
            } else if (u.debug_mode == DEBUG_UV) {
                let uv = computeUV(p, meta);
                if (uvInBounds(uv)) {
                    srcColor = vec3<f32>(uv, 0.5);
                    srcAlpha = 1.0;
                }
            } else if (u.debug_mode == DEBUG_EDGES) {
                let lineWidth = max(u.edge_softness, 1.0);
                let edgeMask = 1.0 - smoothstep(0.0, lineWidth, abs(d));
                srcColor = vec3<f32>(1.0, 0.9, 0.0);
                srcAlpha = edgeMask;
            } else if (meta.render_mode == MODE_OUTLINE) {
                let halfWidth = meta.outline_width * 0.5;
                let ringDist = abs(d) - halfWidth;
                let coverage = clamp(0.5 - ringDist / max(u.edge_softness, 1e-4), 0.0, 1.0);
                srcColor = meta.tint_color.rgb;
                srcAlpha = meta.tint_color.a * coverage;
            } else if (meta.render_mode == MODE_MASK) {
                let coverage = clamp(0.5 - d / max(u.edge_softness, 1e-4), 0.0, 1.0);
                srcColor = meta.tint_color.rgb;
                srcAlpha = meta.tint_color.a * coverage;
            } else {
                // MODE_FILL
                let coverage = clamp(0.5 - d / max(u.edge_softness, 1e-4), 0.0, 1.0);
                if (coverage > 0.0) {
                    let uv = computeUV(p, meta);
                    if (uvInBounds(uv)) {
                        let texColor = textureSampleLevel(srcTexture, srcSampler, uv, 0.0);
                        srcColor = texColor.rgb * meta.tint_color.rgb;
                        srcAlpha = texColor.a * coverage * meta.tint_color.a;
                    }
                }
            }

            if (srcAlpha > 0.0) {
                accumColor = srcColor * srcAlpha + accumColor * (1.0 - srcAlpha);
                accumAlpha = srcAlpha + accumAlpha * (1.0 - srcAlpha);
            }
        }

        workgroupBarrier();
    }

    if (!inBounds) {
        return;
    }

    if (accumAlpha <= 0.0) {
        return; // untouched: outside every polygon, don't write anything
    }

    if (u.debug_mode == DEBUG_NONE && u.output_gamma != 1.0) {
        let invGamma = 1.0 / max(u.output_gamma, 1e-4);
        accumColor = pow(max(accumColor, vec3<f32>(0.0)), vec3<f32>(invGamma));
    }

    textureStore(outTexture, vec2<i32>(gid.xy), vec4<f32>(accumColor, accumAlpha));
}
