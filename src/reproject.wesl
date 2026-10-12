struct Reprojection {
    view_from_clip: mat4x4<f32>,
    target_from_view: mat4x4<f32>,
    size_layer: vec4<u32>,
}
@group(0) @binding(0) var colors: texture_2d_array<f32>;
@group(0) @binding(1) var depths: texture_depth_2d_array;
@group(0) @binding(2) var<uniform> settings: Reprojection;

struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) pixel: vec2<u32>,
}

@vertex
fn vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> Vertex {
    let pixel = vec2<u32>(instance % settings.size_layer.x, instance / settings.size_layer.x);
    let depth = textureLoad(depths, vec2<i32>(pixel), i32(settings.size_layer.z), 0);
    // Each source pixel is a constant-depth quad. Using the four projected
    // corners covers minification/magnification without mixing across a depth
    // boundary. Reverse-Z testing combines both anchors by nearest surface.
    var corner = vec2<f32>(0.0);
    switch vertex {
        case 1u, 4u: { corner = vec2<f32>(1.0, 0.0); }
        case 2u, 3u: { corner = vec2<f32>(0.0, 1.0); }
        case 5u: { corner = vec2<f32>(1.0); }
        default: {}
    }
    let uv = (vec2<f32>(pixel) + corner) / vec2<f32>(settings.size_layer.xy);
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    // Homogeneous reconstruction also handles the cleared/infinite-depth sky.
    let view = settings.view_from_clip * vec4<f32>(ndc, depth, 1.0);
    var result: Vertex;
    result.position = settings.target_from_view * view;
    result.pixel = pixel;
    return result;
}

@fragment
fn fragment(input: Vertex) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(colors, vec2<i32>(input.pixel), i32(settings.size_layer.z), 0).rgb, 1.0);
}
