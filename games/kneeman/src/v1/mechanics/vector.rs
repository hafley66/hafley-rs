use super::{EntityId, GeometryId, KindId, MaterialId, PaintId, ProgramId, ScriptState, SymbolId};
use crate::v1::Vector2;
use crate::v1::geo::Shape;
use serde::{Deserialize, Serialize};

/// SVG-shaped authoring commands. Static content may keep curves; normalized gameplay collision
/// consumes deterministically flattened contours.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum PathCmd {
    MoveTo(Vector2),
    LineTo(Vector2),
    QuadTo {
        ctrl: Vector2,
        to: Vector2,
    },
    CubicTo {
        c1: Vector2,
        c2: Vector2,
        to: Vector2,
    },
    Close,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct JointSpec {
    pub a: u16,
    pub b: u16,
    pub strength: f32,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SlotSpec {
    pub name: SymbolId,
    pub local: Transform2,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct VectorAsset {
    pub commands: Vec<PathCmd>,
    pub fill_rule: FillRule,
    pub joints: Vec<JointSpec>,
    pub slots: Vec<SlotSpec>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PathSpan {
    pub start: u16,
    pub len: u16,
    pub closed: bool,
    pub revision: u16,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum GeometrySpec {
    Asset(GeometryId),
    DynamicPath { capacity: u16, closed: bool },
    Primitive(Shape),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum GeometryRef {
    Asset(GeometryId),
    Dynamic(PathSpan),
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Transform2 {
    pub pos: Vector2,
    pub rot: f32,
    pub scale: Vector2,
}

impl Transform2 {
    pub const IDENTITY: Self = Self {
        pos: Vector2::ZERO,
        rot: 0.0,
        scale: Vector2::ONE,
    };
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BodyMode {
    Static,
    Kinematic,
    Dynamic,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct RigidBody2 {
    pub transform: Transform2,
    pub velocity: Vector2,
    pub angular_velocity: f32,
    pub inv_mass: f32,
    pub inv_inertia: f32,
    pub mode: BodyMode,
}

impl RigidBody2 {
    pub const STATIC: Self = Self {
        transform: Transform2::IDENTITY,
        velocity: Vector2::ZERO,
        angular_velocity: 0.0,
        inv_mass: 0.0,
        inv_inertia: 0.0,
        mode: BodyMode::Static,
    };
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SurfaceGate {
    Solid,
    Soft,
    OneWay { forward: bool },
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PhysicsMaterial {
    pub restitution: f32,
    pub friction: f32,
    pub density: f32,
    pub gravity_scale: f32,
    pub angular_damping: f32,
    pub gate: SurfaceGate,
    pub break_impulse: f32,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PathBuildSpec {
    pub min_segment: f32,
    pub flatten_tolerance: f32,
    pub floor_tolerance: f32,
    pub wall_tolerance: f32,
    pub ledge_curvature: f32,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CollisionRole {
    Surface,
    Sensor,
    Hurt,
    Hit,
    Grab,
    Guard,
    Push,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum ColliderSource {
    Stroke { radius: f32 },
    Fill,
    Primitive(Shape),
    Children,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ColliderSpec {
    pub source: ColliderSource,
    pub role: CollisionRole,
    pub layer: u32,
    pub mask: u32,
    pub material: MaterialId,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Paint {
    pub fill_rgba: Option<u32>,
    pub stroke_rgba: Option<u32>,
    pub stroke_width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    pub z_index: i16,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AnchorMode {
    Position,
    Translation,
    Transform,
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Anchor {
    pub host: EntityId,
    pub local: Transform2,
    pub mode: AnchorMode,
}

/// Immutable prefab/content row. Identity is the external `KindId`; no Rust kind variant exists.
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct VectorBodySpec {
    pub kind: KindId,
    pub geometry: GeometrySpec,
    pub collider: ColliderSpec,
    pub material: MaterialId,
    pub paint: PaintId,
    pub program: ProgramId,
}

/// Rollback instance shape. Storage may later split into parallel arenas; this value pins the
/// semantic decomposition without requiring the physical layout to mimic a Godot node.
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct VectorBodyInstance {
    pub id: EntityId,
    pub kind: KindId,
    pub geometry: GeometryRef,
    pub body: RigidBody2,
    pub collider: ColliderSpec,
    pub material: MaterialId,
    pub paint: PaintId,
    pub program: ProgramId,
    pub script: ScriptState,
    pub anchor: Option<Anchor>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_asset_and_body_instance_roundtrip_without_content_enum() {
        let asset = VectorAsset {
            commands: vec![
                PathCmd::MoveTo(Vector2::ZERO),
                PathCmd::LineTo(Vector2::new(32.0, 0.0)),
                PathCmd::LineTo(Vector2::new(32.0, 32.0)),
                PathCmd::Close,
            ],
            fill_rule: FillRule::NonZero,
            joints: vec![JointSpec {
                a: 0,
                b: 1,
                strength: 40.0,
            }],
            slots: vec![SlotSpec {
                name: SymbolId(9),
                local: Transform2::IDENTITY,
            }],
        };
        let asset_bytes = bincode::serialize(&asset).unwrap();
        let asset_back: VectorAsset = bincode::deserialize(&asset_bytes).unwrap();
        assert_eq!(asset_back, asset);

        let instance = VectorBodyInstance {
            id: EntityId(2),
            kind: KindId(77),
            geometry: GeometryRef::Dynamic(PathSpan {
                start: 12,
                len: 4,
                closed: true,
                revision: 3,
            }),
            body: RigidBody2 {
                mode: BodyMode::Dynamic,
                inv_mass: 0.25,
                ..RigidBody2::STATIC
            },
            collider: ColliderSpec {
                source: ColliderSource::Stroke { radius: 7.0 },
                role: CollisionRole::Surface,
                layer: 1,
                mask: u32::MAX,
                material: MaterialId(4),
            },
            material: MaterialId(4),
            paint: PaintId(5),
            program: ProgramId(6),
            script: ScriptState::EMPTY,
            anchor: None,
        };
        let bytes = bincode::serialize(&instance).unwrap();
        let back: VectorBodyInstance = bincode::deserialize(&bytes).unwrap();
        assert_eq!(back, instance);
    }
}
