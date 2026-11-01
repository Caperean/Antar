struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) i_rect_x: f32,
    @location(2) i_rect_y: f32,
    @location(3) i_rect_w: f32,
    @location(4) i_rect_h: f32,
    @location(5) i_color_r: f32,
    @location(6) i_color_g: f32,
    @location(7) i_color_b: f32,
    @location(8) i_color_a: f32,
    @location(9)  i_ta: f32,
    @location(10) i_tb: f32,
    @location(11) i_tc: f32,
    @location(12) i_td: f32,
    @location(13) i_tx: f32,
    @location(14) i_ty: f32,
};

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var local = vec2<f32>(
        in.i_rect_x + in.pos.x * in.i_rect_w,
        in.i_rect_y + in.pos.y * in.i_rect_h,
    );

    let world = vec2<f32>(
        in.i_ta * local.x + in.i_tc * local.y + in.i_tx,
        in.i_tb * local.x + in.i_td * local.y + in.i_ty,
    );

    let clip = uniforms.proj * vec4<f32>(world, 0.0, 1.0);

    let premult_a = in.i_color_a;
    let color = vec4<f32>(
        in.i_color_r * premult_a,
        in.i_color_g * premult_a,
        in.i_color_b * premult_a,
        premult_a,
    );

    return VertexOutput(clip, color);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
