@group(1) @binding(0) var t_sprite: texture_2d<f32>;
@group(1) @binding(1) var s_sprite: sampler;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) uv:  vec2<f32>,
    @location(2) i_dst: vec4<f32>,             // x, y, w, h
    @location(3) i_src: vec4<f32>,             // x, y, w, h
    @location(4) i_color: vec4<f32>,           // r, g, b, a
    @location(5) i_transform_abcd: vec4<f32>,  // a, b, c, d
    @location(6) i_transform_t: vec2<f32>,     // tx, ty
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let dst = in.i_dst;
    let src = in.i_src;
    let ta = in.i_transform_abcd.x;
    let tb = in.i_transform_abcd.y;
    let tc = in.i_transform_abcd.z;
    let td = in.i_transform_abcd.w;
    let tx = in.i_transform_t.x;
    let ty = in.i_transform_t.y;

    var local = vec2<f32>(
        dst.x + in.pos.x * dst.z,
        dst.y + in.pos.y * dst.w,
    );

    let world = vec2<f32>(
        ta * local.x + tc * local.y + tx,
        tb * local.x + td * local.y + ty,
    );

    let clip = uniforms.proj * vec4<f32>(world, 0.0, 1.0);

    let uv = vec2<f32>(
        src.x + in.uv.x * src.z,
        src.y + in.uv.y * src.w,
    );

    return VertexOutput(clip, uv, in.i_color);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(t_sprite, s_sprite, in.uv);
    // Apply tint: rgb multiplies the sampled texel (white tint = no-op,
    // matching the old behavior when no `color` was passed to drawImage),
    // alpha combines with the texture's own alpha as before.
    color = color * in.color;
    return vec4<f32>(color.rgb * color.a, color.a);
}
