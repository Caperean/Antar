// ============================================================================
// Polygon renderer — per-pixel even-odd SDF over the polygon's bounding box.
//
// This replaces the abandoned compute path (rasterizer.wgsl). That shader
// cannot run as written: it called workgroupBarrier() from control flow where
// out-of-bounds invocations `return`ed and invalid polygons `continue`d past
// the barrier, and barriers must be in uniform control flow. It also targeted
// a storage texture that nothing ever composited onto the surface, and forced
// every polygon into a separate pass, which would have destroyed draw order.
//
// This version draws in the ordinary render pass, so polygons blend with
// sprites and rects through the existing premultiplied-alpha pipeline and keep
// their place in the command stream.
// ============================================================================

const MAX_VERTICES: u32 = 256u;

const MODE_FILL: u32 = 0u;
const MODE_OUTLINE: u32 = 1u;
const MODE_MASK: u32 = 2u;

const DEBUG_NONE: u32 = 0u;
const DEBUG_SDF: u32 = 1u;
const DEBUG_UV: u32 = 2u;
const DEBUG_EDGES: u32 = 3u;

const FLAG_SKIP_TRANSPARENT: u32 = 1u;

const EPSILON: f32 = 1e-4;

// Must match `POLYGON_META_STRIDE` and the flatten in `GpuContext::execute`.
// 24 × f32 = 96 bytes; the trailing four fields are the global polygon
// settings (edge softness, debug mode, gamma, min area), replicated per
// instance so this pipeline needs no extra uniform binding.
struct PolygonMeta {
    vertex_offset : u32,
    vertex_count  : u32,
    translate     : vec2<f32>, // identity — vertices are transformed on CPU
    scale         : vec2<f32>, // identity, see below
    rotation      : f32,       // identity
    render_mode   : u32,
    bbox_min      : vec2<f32>,
    bbox_max      : vec2<f32>,
    outline_width : f32,
    flags         : u32,
    _pad1         : vec2<f32>,
    tint_color    : vec4<f32>,
    edge_softness : f32,
    debug_mode    : u32,
    output_gamma  : f32,
    min_polygon_area_px : f32,
}

@group(1) @binding(0) var<storage, read> polygons : array<PolygonMeta>;
@group(1) @binding(1) var<storage, read> polygon_vertices : array<vec2<f32>>;
@group(2) @binding(0) var t_poly : texture_2d<f32>;
@group(2) @binding(1) var s_poly : sampler;

struct VertexOutput {
    @builtin(position) clip_pos : vec4<f32>,
    @location(0) @interpolate(flat) meta_index : u32,
}

@vertex
fn vs_main(
    @location(0) pos : vec2<f32>,
    @builtin(instance_index) instance : u32,
) -> VertexOutput {
    let poly = polygons[instance];

    // The quad only has to cover the polygon. An outline (or the edge debug
    // view) is drawn outside the contour, so pad the box enough to hold it.
    var pad = 0.0;
    if (poly.render_mode == MODE_OUTLINE) {
        pad = poly.outline_width * 0.5 + poly.edge_softness;
    }
    if (poly.debug_mode == DEBUG_EDGES) {
        pad = max(pad, poly.edge_softness);
    }

    let bmin = poly.bbox_min - vec2<f32>(pad, pad);
    let bmax = poly.bbox_max + vec2<f32>(pad, pad);
    let world = bmin + pos * (bmax - bmin);

    return VertexOutput(uniforms.proj * vec4<f32>(world, 0.0, 1.0), instance);
}

fn sd_segment(p : vec2<f32>, a : vec2<f32>, b : vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), EPSILON), 0.0, 1.0);
    return length(pa - ba * h);
}

// Signed distance to the contour: negative inside, using the even-odd rule.
// The short-circuit on the crossing test keeps the division by (vj.y - vi.y)
// unreachable when the segment is horizontal.
fn signed_distance(p : vec2<f32>, base : u32, count : u32) -> f32 {
    var d : f32 = 1e10;
    var inside : bool = false;
    var j : u32 = count - 1u;

    for (var i : u32 = 0u; i < count; i = i + 1u) {
        let vi = polygon_vertices[base + i];
        let vj = polygon_vertices[base + j];

        d = min(d, sd_segment(p, vi, vj));

        let crosses = ((vi.y > p.y) != (vj.y > p.y)) &&
                      (p.x < (vj.x - vi.x) * (p.y - vi.y) / (vj.y - vi.y) + vi.x);
        if (crosses) {
            inside = !inside;
        }
        j = i;
    }

    return select(d, -d, inside);
}

// Vertices and bbox are both in screen space, so the UV is a direct
// normalisation. `translate`/`scale`/`rotation` are deliberately unused: the
// CPU applies the transform once in `resolve_fill_polygon` and stores identity
// here, so honouring them would transform the geometry a second time.
fn polygon_uv(p : vec2<f32>, poly : PolygonMeta) -> vec2<f32> {
    let size = max(poly.bbox_max - poly.bbox_min, vec2<f32>(EPSILON, EPSILON));
    return (p - poly.bbox_min) / size;
}

fn sdf_debug_color(d : f32) -> vec3<f32> {
    let inside_color = vec3<f32>(0.25, 0.45, 1.0);
    let outside_color = vec3<f32>(1.0, 0.55, 0.15);
    var base = select(outside_color, inside_color, d < 0.0);

    let stripe = abs(fract(d / 10.0) - 0.5) * 2.0;
    base = base * mix(0.35, 1.0, stripe);

    let edge_line = 1.0 - smoothstep(0.0, 1.5, abs(d));
    return mix(base, vec3<f32>(1.0, 1.0, 1.0), edge_line);
}

@fragment
fn fs_main(in : VertexOutput) -> @location(0) vec4<f32> {
    let poly = polygons[in.meta_index];
    let count = poly.vertex_count;

    if (count < 3u || count > MAX_VERTICES) {
        return vec4<f32>(0.0);
    }
    if ((poly.flags & FLAG_SKIP_TRANSPARENT) != 0u) {
        return vec4<f32>(0.0);
    }

    let bbox_size = poly.bbox_max - poly.bbox_min;
    if (bbox_size.x <= EPSILON || bbox_size.y <= EPSILON) {
        return vec4<f32>(0.0);
    }
    if (bbox_size.x * bbox_size.y < poly.min_polygon_area_px) {
        return vec4<f32>(0.0);
    }

    let p = vec2<f32>(in.clip_pos.x, in.clip_pos.y);
    let d = signed_distance(p, poly.vertex_offset, count);
    let softness = max(poly.edge_softness, EPSILON);

    var src_color = vec3<f32>(0.0, 0.0, 0.0);
    var src_alpha = 0.0;

    if (poly.debug_mode == DEBUG_SDF) {
        src_color = sdf_debug_color(d);
        src_alpha = 1.0;
    } else if (poly.debug_mode == DEBUG_UV) {
        src_color = vec3<f32>(polygon_uv(p, poly), 0.5);
        src_alpha = 1.0;
    } else if (poly.debug_mode == DEBUG_EDGES) {
        let width = max(poly.edge_softness, 1.0);
        src_color = vec3<f32>(1.0, 0.9, 0.0);
        src_alpha = 1.0 - smoothstep(0.0, width, abs(d));
    } else if (poly.render_mode == MODE_OUTLINE) {
        let ring = abs(d) - poly.outline_width * 0.5;
        src_color = poly.tint_color.rgb;
        src_alpha = poly.tint_color.a * clamp(0.5 - ring / softness, 0.0, 1.0);
    } else if (poly.render_mode == MODE_MASK) {
        src_color = poly.tint_color.rgb;
        src_alpha = poly.tint_color.a * clamp(0.5 - d / softness, 0.0, 1.0);
    } else {
        let coverage = clamp(0.5 - d / softness, 0.0, 1.0);
        // Explicit LOD: `textureSample` needs uniform control flow for its
        // implicit derivatives, and this branch is not uniform. The texture
        // has no mip levels, so level 0 is the whole image.
        let tex = textureSampleLevel(t_poly, s_poly, polygon_uv(p, poly), 0.0);
        src_color = tex.rgb * poly.tint_color.rgb;
        src_alpha = tex.a * coverage * poly.tint_color.a;
    }

    if (src_alpha <= 0.0) {
        return vec4<f32>(0.0);
    }

    var out_rgb = src_color;
    if (poly.debug_mode == DEBUG_NONE && poly.output_gamma != 1.0) {
        let inv_gamma = 1.0 / max(poly.output_gamma, EPSILON);
        out_rgb = pow(max(out_rgb, vec3<f32>(0.0)), vec3<f32>(inv_gamma));
    }

    // Premultiplied, matching sprite.wgsl and GpuCanvasAlphaMode::Premultiplied.
    return vec4<f32>(out_rgb * src_alpha, src_alpha);
}
