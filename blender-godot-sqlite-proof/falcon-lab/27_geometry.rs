//! World-space line geometry shared by SQL presentation hosts.
use super::boundary::Row;
use cgmath::{Matrix4, Vector3, Vector4};
use parry3d::{
    math::Vec3,
    shape::{Ball, Capsule, Cuboid},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub color: [f32; 4],
    pub width: f32,
}

fn mesh(
    out: &mut Vec<Line>,
    points: &[Vec3],
    indices: &[[u32; 3]],
    matrix: Matrix4<f32>,
    color: [f32; 4],
) {
    let mut edges = std::collections::BTreeSet::new();
    for t in indices {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            edges.insert((a.min(b), a.max(b)));
        }
    }
    let positions: Vec<[f32; 3]> = points
        .iter()
        .map(|p| {
            (matrix * Vector4::new(p.x, p.y, p.z, 1.0))
                .truncate()
                .into()
        })
        .collect();
    for (a, b) in edges {
        out.push(Line {
            a: positions[a as usize],
            b: positions[b as usize],
            color,
            width: 0.85,
        });
    }
}

#[tracing::instrument(target = "falcon::presentation", level = "trace", skip_all, fields(rows = rows.len()))]
pub fn wire(rows: &[Row]) -> Vec<Line> {
    let meta = &rows.iter().find(|r| r.kind == 0).unwrap().values;
    let mut out = Vec::new();
    for z in (-30..140).step_by(10) {
        out.push(Line {
            a: [-12.0, 0.0, z as f32],
            b: [12.0, 0.0, z as f32],
            color: [0.15, 0.24, 0.32, 1.0],
            width: 0.8,
        });
    }
    let root = Matrix4::from_translation(Vector3::new(
        meta[2] as f32,
        (meta[3] + meta[12]) as f32,
        (meta[4] + meta[11]) as f32,
    ));
    for row in rows {
        let v = &row.values;
        match row.kind {
            1 => {
                let (p, i) = Cuboid::new(Vec3::new(3.0, 6.0, 4.0)).to_trimesh();
                let color = if meta[7] >= 0.0 && row.tick - (meta[7] as i64) < 10 {
                    [1.0, 0.42, 0.18, 1.0]
                } else {
                    [0.25, 0.85, 0.9, 1.0]
                };
                mesh(
                    &mut out,
                    &p,
                    &i,
                    Matrix4::from_translation(Vector3::new(v[0] as f32, v[1] as f32, v[2] as f32)),
                    color,
                );
            }
            2 if v[23] != 0.0 => {
                let matrix = Matrix4::from(std::array::from_fn::<_, 4, _>(|col| {
                    std::array::from_fn::<_, 4, _>(|i| v[col * 4 + i] as f32)
                }));
                let (p, i) = Capsule::new(
                    Vec3::new(v[16] as f32, v[17] as f32, v[18] as f32),
                    Vec3::new(v[19] as f32, v[20] as f32, v[21] as f32),
                    v[22] as f32,
                )
                .to_trimesh(8, 4);
                mesh(&mut out, &p, &i, root * matrix, [0.75, 0.45, 1.0, 0.85]);
            }
            3 if v[5] != 0.0 => {
                let (p, i) = Ball::new(v[3] as f32).to_trimesh(12, 8);
                mesh(
                    &mut out,
                    &p,
                    &i,
                    root * Matrix4::from_translation(Vector3::new(
                        v[0] as f32,
                        v[1] as f32,
                        v[2] as f32,
                    )),
                    [1.0, 0.42, 0.18, 1.0],
                );
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn translated_cube_edges_preserve_all_coordinates() {
        let mut out = Vec::new();
        let (p, i) = Cuboid::new(Vec3::new(3.0, 6.0, 4.0)).to_trimesh();
        mesh(
            &mut out,
            &p,
            &i,
            Matrix4::from_translation(Vector3::new(2.0, 3.0, 5.0)),
            [1.0; 4],
        );
        let mut legacy = Vec::new();
        let mut shared = Vec::new();
        use super::super::super::baseline;
        baseline::mesh_wire(
            &mut legacy,
            &p,
            &i,
            Matrix4::from_translation(Vector3::new(2.0, 3.0, 5.0)),
            [1.0; 4],
        );
        for line in &out {
            baseline::gpu::line(
                &mut shared,
                baseline::project(line.a.into()),
                baseline::project(line.b.into()),
                line.width,
                line.color,
            );
        }
        assert_eq!(
            legacy, shared,
            "shared geometry changed the existing wgpu projection"
        );
        let points: Vec<_> = out.iter().flat_map(|l| [l.a, l.b]).collect();
        assert_eq!(out.len(), 18);
        for axis in 0..3 {
            assert_eq!(
                points.iter().map(|p| p[axis]).fold(f32::INFINITY, f32::min),
                [-1.0, -3.0, 1.0][axis]
            );
            assert_eq!(
                points
                    .iter()
                    .map(|p| p[axis])
                    .fold(f32::NEG_INFINITY, f32::max),
                [5.0, 9.0, 9.0][axis]
            );
        }
    }
}
