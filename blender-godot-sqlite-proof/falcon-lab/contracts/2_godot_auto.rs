// Generated from 0_presentation.tsp; sha256:107cb63371f971260fd0828a841cbd8a499b9fb455a6e85f57b3143075da7d26
use godot::prelude::*;

use crate::fixture::sql_viewer::boundary::contracts::*;

impl FixtureStatus {
    pub fn to_dictionary(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        out.set("simulation_tick", self.simulation_tick.to_variant());
        out.set("published_generation", i64::try_from(self.published_generation).expect("Godot integer range").to_variant());
        out.set("renderer_generation", i64::try_from(self.renderer_generation).expect("Godot integer range").to_variant());
        out.set("rows", i64::try_from(self.rows).expect("Godot integer range").to_variant());
        out.set("window_frames", i64::try_from(self.window_frames).expect("Godot integer range").to_variant());
        out.set("held_generation", match self.held_generation { Some(value) => i64::try_from(value).expect("Godot integer range").to_variant(), None => Variant::nil() });
        out.set("held_damage", match self.held_damage { Some(value) => value.to_variant(), None => Variant::nil() });
        out.set("fresh_tick91_damage", match self.fresh_tick91_damage { Some(value) => value.to_variant(), None => Variant::nil() });
        out.set("restored", self.restored.iter().map(|v| i64::from(*v)).collect::<Array<i64>>().to_variant());
        out.set("saved", self.saved.iter().map(|v| i64::from(*v)).collect::<Array<i64>>().to_variant());
        out.set("advances", i64::try_from(self.advances).expect("Godot integer range").to_variant());
        out.set("runtime_next_tick", self.runtime_next_tick.to_variant());
        out.set("input_bits", i64::try_from(self.input_bits).expect("Godot integer range").to_variant());
        out
    }
}

impl ScheduledStatus {
    pub fn to_dictionary(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        out.set("simulation_tick", self.simulation_tick.to_variant());
        out.set("published_tick", self.published_tick.to_variant());
        out.set("generation", i64::try_from(self.generation).expect("Godot integer range").to_variant());
        out.set("skipped_publications", i64::try_from(self.skipped_publications).expect("Godot integer range").to_variant());
        out.set("published", self.published.to_variant());
        out.set("advances", i64::try_from(self.advances).expect("Godot integer range").to_variant());
        out.set("restored", self.restored.iter().map(|v| i64::from(*v)).collect::<Array<i64>>().to_variant());
        out.set("held_generation", match self.held_generation { Some(value) => i64::try_from(value).expect("Godot integer range").to_variant(), None => Variant::nil() });
        out.set("held_damage", match self.held_damage { Some(value) => value.to_variant(), None => Variant::nil() });
        out.set("fresh_tick91_damage", match self.fresh_tick91_damage { Some(value) => value.to_variant(), None => Variant::nil() });
        out.set("rows", i64::try_from(self.rows).expect("Godot integer range").to_variant());
        out.set("window_frames", i64::try_from(self.window_frames).expect("Godot integer range").to_variant());
        out
    }
}

impl ScheduledFrameStatus {
    pub fn to_dictionary(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        out.set("simulation_tick", self.simulation_tick.to_variant());
        out.set("published_tick", self.published_tick.to_variant());
        out.set("generation", i64::try_from(self.generation).expect("Godot integer range").to_variant());
        out.set("skipped_publications", i64::try_from(self.skipped_publications).expect("Godot integer range").to_variant());
        out.set("published", self.published.to_variant());
        out.set("advances", i64::try_from(self.advances).expect("Godot integer range").to_variant());
        out.set("restored", self.restored.iter().map(|v| i64::from(*v)).collect::<Array<i64>>().to_variant());
        out.set("held_generation", match self.held_generation { Some(value) => i64::try_from(value).expect("Godot integer range").to_variant(), None => Variant::nil() });
        out.set("held_damage", match self.held_damage { Some(value) => value.to_variant(), None => Variant::nil() });
        out.set("fresh_tick91_damage", match self.fresh_tick91_damage { Some(value) => value.to_variant(), None => Variant::nil() });
        out.set("rows", i64::try_from(self.rows).expect("Godot integer range").to_variant());
        out.set("window_frames", i64::try_from(self.window_frames).expect("Godot integer range").to_variant());
        out.set("renderer_generation", i64::try_from(self.renderer_generation).expect("Godot integer range").to_variant());
        out.set("observed_simulation_tick", self.observed_simulation_tick.to_variant());
        out
    }
}

impl ExternalStatus {
    pub fn to_dictionary(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        out.set("source_pid", i64::try_from(self.source_pid).expect("Godot integer range").to_variant());
        out.set("source_generation", i64::try_from(self.source_generation).expect("Godot integer range").to_variant());
        out.set("renderer_generation", i64::try_from(self.renderer_generation).expect("Godot integer range").to_variant());
        out.set("published_tick", self.published_tick.to_variant());
        out.set("source_elapsed_us", i64::try_from(self.source_elapsed_us).expect("Godot integer range").to_variant());
        out.set("skipped_generations", i64::try_from(self.skipped_generations).expect("Godot integer range").to_variant());
        out.set("consumer_pid", i64::try_from(self.consumer_pid).expect("Godot integer range").to_variant());
        out.set("ipc_sql_exact", self.ipc_sql_exact.to_variant());
        out
    }
}

pub struct GodotFramePayload {
    pub rows: PackedFloat64Array,
    pub vertices: PackedVector3Array,
    pub colors: PackedColorArray,
    pub status: FrameStatus,
}

impl GodotFramePayload {
    pub fn to_dictionary(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        out.set("rows", self.rows.to_variant());
        out.set("vertices", self.vertices.to_variant());
        out.set("colors", self.colors.to_variant());
        out.set("status", self.status.to_dictionary().to_variant());
        out
    }
}

pub struct GodotMeshReceipt {
    pub generation: i64,
    pub rows: PackedFloat64Array,
    pub vertices: PackedVector3Array,
}

impl GodotMeshReceipt {
    pub fn to_dictionary(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        out.set("generation", self.generation.to_variant());
        out.set("rows", self.rows.to_variant());
        out.set("vertices", self.vertices.to_variant());
        out
    }
}

impl GodotMeshReceipt {
    pub fn from_dictionary(value: &VarDictionary) -> Self {
        let out = Self {
            generation: value.get("generation").expect("missing generation").try_to::<i64>().expect("invalid generation"),
            rows: value.get("rows").expect("missing rows").try_to::<PackedFloat64Array>().expect("invalid rows"),
            vertices: value.get("vertices").expect("missing vertices").try_to::<PackedVector3Array>().expect("invalid vertices"),
        };
        assert!(out.rows.len() <= 27648, "oversized rows");
        assert!(out.vertices.len() <= 100000, "oversized vertices");
        out
    }
}

impl FrameStatus {
    pub fn to_dictionary(&self) -> VarDictionary {
        match self {
            Self::Fixture(value) => value.to_dictionary(),
            Self::Scheduled(value) => value.to_dictionary(),
            Self::External(value) => value.to_dictionary(),
        }
    }
    pub fn renderer_generation(&self) -> u64 {
        match self {
            Self::Fixture(value) => value.renderer_generation,
            Self::Scheduled(value) => value.renderer_generation,
            Self::External(value) => value.renderer_generation,
        }
    }
}
