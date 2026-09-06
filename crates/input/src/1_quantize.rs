/// Quantize a normalized device axis into the complete signed rollback range.
pub fn quantize_axis(value: f32) -> i8 {
    (value.clamp(-1.0, 1.0) * 127.0) as i8
}

/// Recover the normalized value represented by a quantized rollback axis.
pub fn dequantize_axis(value: i8) -> f32 {
    f32::from(value) / 127.0
}
