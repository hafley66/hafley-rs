//! Comparison presentation primitives; labels and scenario expectations stay with the game.
use crate::{Vertex, line};
use font8x8::UnicodeFonts;

pub fn text(out: &mut Vec<Vertex>, label: &str, x: f32, y: f32, scale: f32, color: [f32; 4]) {
    for (i, c) in label.chars().enumerate() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(c) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        let x = x + (i * 8 + col) as f32 * scale;
                        let y = y + row as f32 * scale;
                        line(
                            out,
                            [x, y + scale * 0.5],
                            [x + scale, y + scale * 0.5],
                            scale,
                            color,
                        );
                    }
                }
            }
        }
    }
}

/// Place normalized full-width geometry into one of N horizontal panels.
pub fn panel(vertices: &mut [Vertex], index: usize, count: usize) {
    assert!(count > 0 && index < count);
    for v in vertices {
        v[0] = (v[0] + 1.0) / count as f32 - 1.0 + 2.0 * index as f32 / count as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_mapping_and_text_translation() {
        let mut mesh = [
            [-1.0, 0.5, 1.0, 0.0, 0.0, 1.0],
            [1.0, -0.5, 0.0, 1.0, 0.0, 1.0],
        ];
        panel(&mut mesh, 1, 2);
        assert_eq!(
            mesh,
            [
                [0.0, 0.5, 1.0, 0.0, 0.0, 1.0],
                [1.0, -0.5, 0.0, 1.0, 0.0, 1.0]
            ]
        );
        let mut a = Vec::new();
        text(&mut a, "A", 0.0, 0.0, 1.0, [1.0; 4]);
        let mut b = Vec::new();
        text(&mut b, "AA", 0.0, 0.0, 1.0, [1.0; 4]);
        assert!(!a.is_empty());
        assert_eq!(b.len(), a.len() * 2);
        assert_eq!(&b[..a.len()], a);
        for (left, right) in a.iter().zip(&b[a.len()..]) {
            assert!((right[0] - left[0] - 16.0 / crate::WIDTH as f32).abs() < 0.000001);
            assert_eq!(&left[1..], &right[1..]);
        }
    }
}
