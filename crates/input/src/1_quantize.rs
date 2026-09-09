// Ported from hafley-rs-game-runtime crates/input at 8646fa2 (MIT OR Apache-2.0).
pub fn quantize_axis(value: f32) -> i8 {
    (value.clamp(-1.0, 1.0) * 127.0) as i8
}
pub fn dequantize_axis(value: i8) -> f32 {
    f32::from(value) / 127.0
}
