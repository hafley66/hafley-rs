use parry3d::{math::{Pose, Vec3}, query, shape::{Capsule, SharedShape}};
use serde::{Deserialize, Serialize};

pub const FPS: u32 = 60;
pub const FRAME_COUNT: u32 = 180;
pub const MOVING_RADIUS: f32 = 0.2;
pub const TARGET_RADIUS: f32 = 0.25;
pub const TARGET_A: [f32; 3] = [1.0, 0.3, 0.0];
pub const TARGET_B: [f32; 3] = [1.0, 1.3, 0.0];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceFrame {
    pub tick: u32,
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub radius: f32,
    pub hit: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trace {
    pub fps: u32,
    pub target_a: [f32; 3],
    pub target_b: [f32; 3],
    pub target_radius: f32,
    pub frames: Vec<TraceFrame>,
}

fn capsule(a: [f32; 3], b: [f32; 3], radius: f32) -> Capsule {
    Capsule::new(Vec3::from(a), Vec3::from(b), radius)
}

pub fn generate_trace() -> Trace {
    let target = capsule(TARGET_A, TARGET_B, TARGET_RADIUS);
    let frames = (0..FRAME_COUNT).map(|tick| {
        let t = tick as f32 / (FRAME_COUNT - 1) as f32;
        let x = -1.0 + 3.0 * t;
        let y = 0.8 + (t * std::f32::consts::TAU).sin() * 0.15;
        let a = [x, y - 0.45, 0.0];
        let b = [x, y + 0.45, 0.0];
        let moving = capsule(a, b, MOVING_RADIUS);
        let hit = query::intersection_test(
            &Pose::IDENTITY, &moving,
            &Pose::IDENTITY, &target,
        ).expect("capsule/capsule is supported");
        TraceFrame { tick, a, b, radius: MOVING_RADIUS, hit }
    }).collect();
    Trace { fps: FPS, target_a: TARGET_A, target_b: TARGET_B, target_radius: TARGET_RADIUS, frames }
}

pub fn shared_capsule(a: [f32; 3], b: [f32; 3], radius: f32) -> SharedShape {
    SharedShape::new(capsule(a, b, radius))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_is_actual_capsule_query_and_crosses_target() {
        let trace = generate_trace();
        assert_eq!(trace.frames.len(), 180);
        assert!(!trace.frames[0].hit);
        assert!(trace.frames.iter().any(|f| f.hit));
        assert!(!trace.frames[179].hit);
        for frame in &trace.frames {
            let moving = capsule(frame.a, frame.b, frame.radius);
            let target = capsule(trace.target_a, trace.target_b, trace.target_radius);
            assert_eq!(frame.hit, query::intersection_test(&Pose::IDENTITY, &moving, &Pose::IDENTITY, &target).unwrap());
        }
    }
}
