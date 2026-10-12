@group(0) @binding(0) var source: texture_2d<f32>;
@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * 2.0 - vec2<f32>(1.0), 0.0, 1.0);
}
@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    // Tonemapping already ran on this view. The sRGB render attachment encodes
    // the final linear SDR values; it is never a raw float-to-u8 texture copy.
    return vec4<f32>(textureLoad(source, vec2<i32>(position.xy), 0).rgb, 1.0);
}
