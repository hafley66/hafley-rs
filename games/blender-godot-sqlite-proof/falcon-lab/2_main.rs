use base64::Engine;
use brawllib_rs::high_level_fighter::{CollisionBoxValues, HighLevelFrame, HighLevelSubaction};
use cgmath::{Matrix4, Vector3};
use font8x8::UnicodeFonts;
use parry3d::{
    math::{Pose, Vec3},
    query,
    shape::{Ball, Capsule, Cuboid},
};
use serde_json::json;

#[path = "1_gpu.rs"]
pub(crate) mod gpu;
type Error = Box<dyn std::error::Error>;
const FAIR_START: usize = 78;
const TICKS: usize = 180;
const TARGET: [f32; 3] = [0.0, 24.0, 28.0];
const HALF: [f32; 3] = [3.0, 6.0, 4.0];
const PURPLE: [f32; 4] = [0.75, 0.45, 1.0, 0.85];
const CYAN: [f32; 4] = [0.25, 0.85, 0.9, 1.0];
const ORANGE: [f32; 4] = [1.0, 0.42, 0.18, 1.0];

pub(crate) fn load() -> Result<Vec<HighLevelSubaction>, Error> {
    load_files(&[
        "4_pm36_Wait1.html",
        "5_pm36_JumpF.html",
        "1_pm36_AttackAirF.html",
    ])
}

pub(crate) fn load_controlled() -> Result<Vec<HighLevelSubaction>, Error> {
    let actions = load_files(&[
        "4_pm36_Wait1.html", "5_pm36_JumpF.html", "1_pm36_AttackAirF.html",
        "6_pm36_JumpSquat.html", "7_pm36_Fall.html", "8_pm36_LandingAirF.html",
        "9_pm36_LandingHeavy.html",
    ])?;
    assert_eq!(actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["Wait1", "JumpF", "AttackAirF", "JumpSquat", "Fall", "LandingAirF", "LandingHeavy"]);
    assert!(actions.iter().all(|a| !a.bad_interrupts));
    assert_eq!(actions[2].landing_lag, Some(actions[5].frames.len() as f32));
    Ok(actions)
}

fn load_files(files: &[&str]) -> Result<Vec<HighLevelSubaction>, Error> {
    files.iter()
    .map(|file| {
        let html = std::fs::read_to_string(format!(
            "{}/../fixtures/falcon/{file}",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        let payload = html
            .split("const fighter_subaction_data = \"")
            .nth(1)
            .ok_or("missing data")?
            .split('"')
            .next()
            .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD.decode(payload)?;
        let (action, used): (HighLevelSubaction, usize) =
            bincode::serde::decode_from_slice(&bytes, bincode::config::standard())?;
        assert_eq!(used, bytes.len(), "schema must consume the entire payload");
        eprintln!(
            "DECODE {}: {} frames, {} bytes",
            action.name,
            action.frames.len(),
            used
        );
        Ok(action)
    })
    .collect()
}

pub(crate) use falcon_simulation::Tick;

fn simulate(actions: &[HighLevelSubaction], target: [f32; 3]) -> Vec<Tick> {
    let mut damage = 0.0;
    let mut attack_hit = false;
    (0..TICKS)
        .map(|tick| {
            let (action, frame) = if tick < 60 {
                (0, tick % actions[0].frames.len())
            } else if tick < FAIR_START {
                (1, tick - 60)
            } else if tick < FAIR_START + actions[2].frames.len() {
                (2, tick - FAIR_START)
            } else {
                (
                    0,
                    (tick - FAIR_START - actions[2].frames.len()) % actions[0].frames.len(),
                )
            };
            let air = tick.saturating_sub(60) as f32;
            // Deliberate fixture trajectory, independent of PM movement rules.
            let root = [
                0.0,
                (0.95 * air - 0.018 * air * air).max(0.0),
                -12.0 + (0.9 * air).min(32.0),
            ];
            let source = &actions[action].frames[frame];
            let mut hit = None;
            let mut contact = false;
            for hb in &source.hit_boxes {
                let CollisionBoxValues::Hit(values) = &hb.next_values else {
                    continue;
                };
                if !values.enabled || !values.aerial {
                    continue;
                }
                let p = hb.next_pos;
                let center = [
                    p.x + root[0],
                    p.y + root[1] + source.y_pos,
                    p.z + root[2] + source.x_pos,
                ];
                let overlap = query::intersection_test(
                    &Pose::translation(center[0], center[1], center[2]),
                    &Ball::new(hb.next_size),
                    &Pose::translation(target[0], target[1], target[2]),
                    &Cuboid::new(Vec3::from(HALF)),
                )
                .unwrap();
                contact |= overlap;
                if overlap && !attack_hit {
                    damage += values.damage;
                    attack_hit = true;
                    hit = Some((hb.hitbox_id, values.damage));
                }
            }
            Tick {
                action,
                frame,
                root,
                damage,
                contact,
                hit,
            }
        })
        .collect()
}

pub(crate) fn project(p: Vector3<f32>) -> [f32; 2] {
    // Oblique orthographic camera. Source axes: x depth, y up, z forward.
    [
        350.0 + (p.z + p.x * 0.28) * 10.0,
        444.0 - (p.y + p.x * 0.10) * 10.0,
    ]
}

pub(crate) fn mesh_wire(
    out: &mut Vec<gpu::Vertex>,
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
    let positions: Vec<_> = points
        .iter()
        .map(|p| project((matrix * cgmath::Vector4::new(p.x, p.y, p.z, 1.0)).truncate()))
        .collect();
    for (a, b) in edges {
        gpu::line(
            out,
            positions[a as usize],
            positions[b as usize],
            0.85,
            color,
        );
    }
}

pub(crate) fn text(
    out: &mut Vec<gpu::Vertex>,
    label: &str,
    x: f32,
    y: f32,
    scale: f32,
    color: [f32; 4],
) {
    for (i, c) in label.chars().enumerate() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(c) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        let x = x + (i * 8 + col) as f32 * scale;
                        let y = y + row as f32 * scale;
                        gpu::line(
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

pub(crate) fn draw(
    source: &HighLevelFrame,
    state: &Tick,
    tick: usize,
    slow: bool,
    last_hit: Option<usize>,
    hud: bool,
    target: [f32; 3],
) -> Vec<gpu::Vertex> {
    let mut out = Vec::new();
    let dim = [0.15, 0.24, 0.32, 1.0];
    for x in (-30..60).step_by(5) {
        gpu::line(
            &mut out,
            project(Vector3::new(-12.0, 0.0, x as f32)),
            project(Vector3::new(12.0, 0.0, x as f32)),
            0.8,
            dim,
        );
    }
    for depth in [-12.0, -6.0, 0.0, 6.0, 12.0] {
        gpu::line(
            &mut out,
            project(Vector3::new(depth, 0.0, -30.0)),
            project(Vector3::new(depth, 0.0, 60.0)),
            0.8,
            dim,
        );
    }
    let flash = last_hit.is_some_and(|hit_tick| tick < hit_tick + 10);
    let target_color = if flash { ORANGE } else { CYAN };
    let (vertices, indices) = Cuboid::new(Vec3::from(HALF)).to_trimesh();
    mesh_wire(
        &mut out,
        &vertices,
        &indices,
        Matrix4::from_translation(Vector3::from(target)),
        target_color,
    );
    let label = project(Vector3::new(target[0], target[1] + 9.0, target[2] - 5.0));
    text(&mut out, "SANDBAG", label[0], label[1], 1.5, CYAN);
    text(
        &mut out,
        &format!("{:.0}%", state.damage),
        label[0],
        label[1] + 22.0,
        3.0,
        target_color,
    );
    let root = Matrix4::from_translation(Vector3::new(
        state.root[0],
        state.root[1] + source.y_pos,
        state.root[2] + source.x_pos,
    ));
    for hurt in &source.hurt_boxes {
        if !hurt.hurt_box.enabled {
            continue;
        }
        let a = hurt.hurt_box.offset;
        let b = hurt.hurt_box.stretch;
        let capsule = Capsule::new(
            Vec3::new(a.x, a.y, a.z),
            Vec3::new(b.x, b.y, b.z),
            hurt.hurt_box.radius,
        );
        let (vertices, indices) = capsule.to_trimesh(8, 4);
        mesh_wire(
            &mut out,
            &vertices,
            &indices,
            root * hurt.bone_matrix,
            PURPLE,
        );
    }
    for hb in &source.hit_boxes {
        let CollisionBoxValues::Hit(values) = &hb.next_values else {
            continue;
        };
        if !values.enabled {
            continue;
        }
        let (vertices, indices) = Ball::new(hb.next_size).to_trimesh(12, 8);
        mesh_wire(
            &mut out,
            &vertices,
            &indices,
            root * Matrix4::from_translation(Vector3::new(
                hb.next_pos.x,
                hb.next_pos.y,
                hb.next_pos.z,
            )),
            ORANGE,
        );
    }
    if !hud {
        return out;
    }
    let white = [0.85, 0.91, 0.98, 1.0];
    text(
        &mut out,
        "PM 3.6 FALCON / RUST + WGPU",
        28.0,
        24.0,
        2.5,
        white,
    );
    text(
        &mut out,
        if slow { "REPLAY 0.5X" } else { "FIXTURE 60HZ" },
        28.0,
        58.0,
        1.5,
        CYAN,
    );
    text(
        &mut out,
        &format!(
            "{}  FRAME {:02}   TICK {:03}",
            ["IDLE", "JUMP", "FAIR / KNEE"][state.action],
            state.frame + 1,
            tick
        ),
        28.0,
        86.0,
        1.5,
        white,
    );
    text(
        &mut out,
        "PURPLE: PM HURT VOLUMES   ORANGE: ATTACK HITBOXES",
        28.0,
        482.0,
        1.5,
        PURPLE,
    );
    text(
        &mut out,
        "SCRIPTED TRAVEL / REAL PM POSES / PARRY CONTACT",
        28.0,
        506.0,
        1.5,
        white,
    );
    if last_hit.is_some() {
        text(
            &mut out,
            &format!("HIT +{:.0} / ONCE", state.damage),
            690.0,
            440.0,
            1.8,
            ORANGE,
        );
    }
    out
}

fn verify(actions: &[HighLevelSubaction], trace: &[Tick]) {
    assert_eq!(
        actions.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["Wait1", "JumpF", "AttackAirF"]
    );
    assert_eq!(
        actions.iter().map(|a| a.frames.len()).collect::<Vec<_>>(),
        [61, 36, 40]
    );
    assert_eq!(trace, simulate(actions, TARGET));
    let hits: Vec<_> = trace
        .iter()
        .enumerate()
        .filter(|(_, s)| s.hit.is_some())
        .collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, FAIR_START + 13);
    assert_eq!(hits[0].1.hit, Some((0, 18.0)));
    assert!(trace.iter().filter(|s| s.contact).count() > 1);
    assert_eq!(trace.last().unwrap().damage, 18.0);
    assert!(
        simulate(actions, [0.0, 24.0, 200.0])
            .iter()
            .all(|s| s.damage == 0.0 && !s.contact)
    );
    assert!(trace[..FAIR_START + 13].iter().all(|s| s.damage == 0.0));
    eprintln!(
        "SIM_OK hit_tick={} fair_frame=14 damage=18 applications=1 contact_ticks={}",
        hits[0].0,
        trace.iter().filter(|s| s.contact).count()
    );
}

#[path = "0_tracing.rs"]
pub(crate) mod telemetry;

fn main() -> Result<(), Error> {
    telemetry::init();
    let actions = load()?;
    let trace = simulate(&actions, TARGET);
    verify(&actions, &trace);
    let report = json!({"fps":60,"ticks":TICKS,"target":TARGET,"target_half_extents":HALF,
        "fixture":"Scripted travel and action schedule; extracted PM poses and damage; discrete sphere/box collision",
        "frames":trace.iter().enumerate().map(|(i,s)|json!({"tick":i,"action":actions[s.action].name,"frame":s.frame+1,"root":s.root,"damage":s.damage,"contact":s.contact,"hit":s.hit})).collect::<Vec<_>>()});
    std::fs::write("4_trace.json", serde_json::to_vec_pretty(&report)?)?;
    if std::env::args().any(|arg| arg == "--verify-only") {
        return Ok(());
    }
    let mut capture = gpu::Capture::new("5_falcon_knee.mp4")?;
    for slow in [false, true] {
        let mut last_hit = None;
        for (tick, state) in trace.iter().enumerate() {
            if state.hit.is_some() {
                last_hit = Some(tick);
            }
            let vertices = draw(
                &actions[state.action].frames[state.frame],
                state,
                tick,
                slow,
                last_hit,
                true,
                TARGET,
            );
            for _ in 0..if slow { 2 } else { 1 } {
                capture.frame(&vertices)?;
            }
        }
    }
    capture.finish()
}

#[cfg(test)]
mod tests {
    #[test]
    fn pm_decode_contact_damage_and_replay() {
        let actions = super::load().unwrap();
        let trace = super::simulate(&actions, super::TARGET);
        super::verify(&actions, &trace);
    }
}
