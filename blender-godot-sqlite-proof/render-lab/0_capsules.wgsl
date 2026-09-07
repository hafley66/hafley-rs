struct Frame { a: vec4<f32>, b: vec4<f32> }
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read_write> pixels: array<u32>;

fn capsule(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 0.000001), 0.0, 1.0);
    return length(p - a - t * ab) - r;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= 640u || id.y >= 480u { return; }
    let p = vec2<f32>(f32(id.x) / 640.0 * 4.8 - 1.9,
                      (1.0 - f32(id.y) / 480.0) * 3.6 - 0.8);
    var rgb = vec3<u32>(24u, 35u, 51u);
    if capsule(p, vec2<f32>(1.0, 0.3), vec2<f32>(1.0, 1.3), 0.25) <= 0.0 {
        rgb = vec3<u32>(67u, 162u, 206u);
    }
    if capsule(p, frame.a.xy, frame.b.xy, frame.a.w) <= 0.0 {
        rgb = select(vec3<u32>(245u, 195u, 76u), vec3<u32>(255u, 101u, 68u), frame.b.w > 0.5);
    }
    pixels[id.y * 640u + id.x] = rgb.x | (rgb.y << 8u) | (rgb.z << 16u) | (255u << 24u);
}
