// Shared uniforms for Antar 2D pipelines.
struct Uniforms {
    // Column-major projection packed into a mat4x4 for alignment.
    proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
