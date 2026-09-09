# Generated from 0_presentation.tsp; sha256:055253d52b18369f5035b4ec7038efff8d8212f9988b5d260e3e715865b0e190
extends RefCounted

const BUFFER_FLAG = "--buffer-proof"
const CONTROL_ACTION_LABELS = "IDLE|JUMP|FAIR|JUMP SQUAT|FALL|FAIR LANDING|HEAVY LANDING"
const REPEAT_FLAG = "--repeat"

class FixtureStatus:
	var simulation_tick: int
	var published_generation: int
	var renderer_generation: int
	var rows: int
	var window_frames: int
	var held_generation: Variant
	var held_damage: Variant
	var fresh_tick91_damage: Variant
	var restored: Array
	var saved: Array
	var advances: int
	var runtime_next_tick: int
	var input_bits: int

	static func from_wire(data: Dictionary) -> FixtureStatus:
		var out := FixtureStatus.new()
		assert(data.has("simulation_tick"), "Missing FixtureStatus.simulation_tick")
		assert(typeof(data["simulation_tick"]) == TYPE_INT, "Invalid FixtureStatus.simulation_tick")
		out.simulation_tick = data["simulation_tick"]
		assert(data.has("published_generation"), "Missing FixtureStatus.published_generation")
		assert(typeof(data["published_generation"]) == TYPE_INT, "Invalid FixtureStatus.published_generation")
		out.published_generation = data["published_generation"]
		assert(data.has("renderer_generation"), "Missing FixtureStatus.renderer_generation")
		assert(typeof(data["renderer_generation"]) == TYPE_INT, "Invalid FixtureStatus.renderer_generation")
		out.renderer_generation = data["renderer_generation"]
		assert(data.has("rows"), "Missing FixtureStatus.rows")
		assert(typeof(data["rows"]) == TYPE_INT, "Invalid FixtureStatus.rows")
		out.rows = data["rows"]
		assert(data.has("window_frames"), "Missing FixtureStatus.window_frames")
		assert(typeof(data["window_frames"]) == TYPE_INT, "Invalid FixtureStatus.window_frames")
		out.window_frames = data["window_frames"]
		assert(data.has("held_generation"), "Missing FixtureStatus.held_generation")
		assert(data["held_generation"] == null or typeof(data["held_generation"]) == TYPE_INT, "Invalid FixtureStatus.held_generation")
		out.held_generation = data["held_generation"]
		assert(data.has("held_damage"), "Missing FixtureStatus.held_damage")
		assert(data["held_damage"] == null or typeof(data["held_damage"]) == TYPE_FLOAT, "Invalid FixtureStatus.held_damage")
		out.held_damage = data["held_damage"]
		assert(data.has("fresh_tick91_damage"), "Missing FixtureStatus.fresh_tick91_damage")
		assert(data["fresh_tick91_damage"] == null or typeof(data["fresh_tick91_damage"]) == TYPE_FLOAT, "Invalid FixtureStatus.fresh_tick91_damage")
		out.fresh_tick91_damage = data["fresh_tick91_damage"]
		assert(data.has("restored"), "Missing FixtureStatus.restored")
		assert(data["restored"].size() <= 1024, "Oversized FixtureStatus.restored")
		assert(typeof(data["restored"]) == TYPE_ARRAY, "Invalid FixtureStatus.restored")
		out.restored = data["restored"]
		assert(data.has("saved"), "Missing FixtureStatus.saved")
		assert(data["saved"].size() <= 1024, "Oversized FixtureStatus.saved")
		assert(typeof(data["saved"]) == TYPE_ARRAY, "Invalid FixtureStatus.saved")
		out.saved = data["saved"]
		assert(data.has("advances"), "Missing FixtureStatus.advances")
		assert(typeof(data["advances"]) == TYPE_INT, "Invalid FixtureStatus.advances")
		out.advances = data["advances"]
		assert(data.has("runtime_next_tick"), "Missing FixtureStatus.runtime_next_tick")
		assert(typeof(data["runtime_next_tick"]) == TYPE_INT, "Invalid FixtureStatus.runtime_next_tick")
		out.runtime_next_tick = data["runtime_next_tick"]
		assert(data.has("input_bits"), "Missing FixtureStatus.input_bits")
		assert(typeof(data["input_bits"]) == TYPE_INT, "Invalid FixtureStatus.input_bits")
		out.input_bits = data["input_bits"]
		return out

	func to_wire() -> Dictionary:
		return {
			"simulation_tick": simulation_tick,
			"published_generation": published_generation,
			"renderer_generation": renderer_generation,
			"rows": rows,
			"window_frames": window_frames,
			"held_generation": held_generation,
			"held_damage": held_damage,
			"fresh_tick91_damage": fresh_tick91_damage,
			"restored": restored,
			"saved": saved,
			"advances": advances,
			"runtime_next_tick": runtime_next_tick,
			"input_bits": input_bits,
		}

class ScheduledStatus:
	var simulation_tick: int
	var published_tick: int
	var generation: int
	var skipped_publications: int
	var published: bool
	var advances: int
	var restored: Array
	var held_generation: Variant
	var held_damage: Variant
	var fresh_tick91_damage: Variant
	var rows: int
	var window_frames: int

	static func from_wire(data: Dictionary) -> ScheduledStatus:
		var out := ScheduledStatus.new()
		assert(data.has("simulation_tick"), "Missing ScheduledStatus.simulation_tick")
		assert(typeof(data["simulation_tick"]) == TYPE_INT, "Invalid ScheduledStatus.simulation_tick")
		out.simulation_tick = data["simulation_tick"]
		assert(data.has("published_tick"), "Missing ScheduledStatus.published_tick")
		assert(typeof(data["published_tick"]) == TYPE_INT, "Invalid ScheduledStatus.published_tick")
		out.published_tick = data["published_tick"]
		assert(data.has("generation"), "Missing ScheduledStatus.generation")
		assert(typeof(data["generation"]) == TYPE_INT, "Invalid ScheduledStatus.generation")
		out.generation = data["generation"]
		assert(data.has("skipped_publications"), "Missing ScheduledStatus.skipped_publications")
		assert(typeof(data["skipped_publications"]) == TYPE_INT, "Invalid ScheduledStatus.skipped_publications")
		out.skipped_publications = data["skipped_publications"]
		assert(data.has("published"), "Missing ScheduledStatus.published")
		assert(typeof(data["published"]) == TYPE_BOOL, "Invalid ScheduledStatus.published")
		out.published = data["published"]
		assert(data.has("advances"), "Missing ScheduledStatus.advances")
		assert(typeof(data["advances"]) == TYPE_INT, "Invalid ScheduledStatus.advances")
		out.advances = data["advances"]
		assert(data.has("restored"), "Missing ScheduledStatus.restored")
		assert(data["restored"].size() <= 1024, "Oversized ScheduledStatus.restored")
		assert(typeof(data["restored"]) == TYPE_ARRAY, "Invalid ScheduledStatus.restored")
		out.restored = data["restored"]
		assert(data.has("held_generation"), "Missing ScheduledStatus.held_generation")
		assert(data["held_generation"] == null or typeof(data["held_generation"]) == TYPE_INT, "Invalid ScheduledStatus.held_generation")
		out.held_generation = data["held_generation"]
		assert(data.has("held_damage"), "Missing ScheduledStatus.held_damage")
		assert(data["held_damage"] == null or typeof(data["held_damage"]) == TYPE_FLOAT, "Invalid ScheduledStatus.held_damage")
		out.held_damage = data["held_damage"]
		assert(data.has("fresh_tick91_damage"), "Missing ScheduledStatus.fresh_tick91_damage")
		assert(data["fresh_tick91_damage"] == null or typeof(data["fresh_tick91_damage"]) == TYPE_FLOAT, "Invalid ScheduledStatus.fresh_tick91_damage")
		out.fresh_tick91_damage = data["fresh_tick91_damage"]
		assert(data.has("rows"), "Missing ScheduledStatus.rows")
		assert(typeof(data["rows"]) == TYPE_INT, "Invalid ScheduledStatus.rows")
		out.rows = data["rows"]
		assert(data.has("window_frames"), "Missing ScheduledStatus.window_frames")
		assert(typeof(data["window_frames"]) == TYPE_INT, "Invalid ScheduledStatus.window_frames")
		out.window_frames = data["window_frames"]
		return out

	func to_wire() -> Dictionary:
		return {
			"simulation_tick": simulation_tick,
			"published_tick": published_tick,
			"generation": generation,
			"skipped_publications": skipped_publications,
			"published": published,
			"advances": advances,
			"restored": restored,
			"held_generation": held_generation,
			"held_damage": held_damage,
			"fresh_tick91_damage": fresh_tick91_damage,
			"rows": rows,
			"window_frames": window_frames,
		}

class ScheduledFrameStatus:
	var simulation_tick: int
	var published_tick: int
	var generation: int
	var skipped_publications: int
	var published: bool
	var advances: int
	var restored: Array
	var held_generation: Variant
	var held_damage: Variant
	var fresh_tick91_damage: Variant
	var rows: int
	var window_frames: int
	var renderer_generation: int
	var observed_simulation_tick: int

	static func from_wire(data: Dictionary) -> ScheduledFrameStatus:
		var out := ScheduledFrameStatus.new()
		assert(data.has("simulation_tick"), "Missing ScheduledFrameStatus.simulation_tick")
		assert(typeof(data["simulation_tick"]) == TYPE_INT, "Invalid ScheduledFrameStatus.simulation_tick")
		out.simulation_tick = data["simulation_tick"]
		assert(data.has("published_tick"), "Missing ScheduledFrameStatus.published_tick")
		assert(typeof(data["published_tick"]) == TYPE_INT, "Invalid ScheduledFrameStatus.published_tick")
		out.published_tick = data["published_tick"]
		assert(data.has("generation"), "Missing ScheduledFrameStatus.generation")
		assert(typeof(data["generation"]) == TYPE_INT, "Invalid ScheduledFrameStatus.generation")
		out.generation = data["generation"]
		assert(data.has("skipped_publications"), "Missing ScheduledFrameStatus.skipped_publications")
		assert(typeof(data["skipped_publications"]) == TYPE_INT, "Invalid ScheduledFrameStatus.skipped_publications")
		out.skipped_publications = data["skipped_publications"]
		assert(data.has("published"), "Missing ScheduledFrameStatus.published")
		assert(typeof(data["published"]) == TYPE_BOOL, "Invalid ScheduledFrameStatus.published")
		out.published = data["published"]
		assert(data.has("advances"), "Missing ScheduledFrameStatus.advances")
		assert(typeof(data["advances"]) == TYPE_INT, "Invalid ScheduledFrameStatus.advances")
		out.advances = data["advances"]
		assert(data.has("restored"), "Missing ScheduledFrameStatus.restored")
		assert(data["restored"].size() <= 1024, "Oversized ScheduledFrameStatus.restored")
		assert(typeof(data["restored"]) == TYPE_ARRAY, "Invalid ScheduledFrameStatus.restored")
		out.restored = data["restored"]
		assert(data.has("held_generation"), "Missing ScheduledFrameStatus.held_generation")
		assert(data["held_generation"] == null or typeof(data["held_generation"]) == TYPE_INT, "Invalid ScheduledFrameStatus.held_generation")
		out.held_generation = data["held_generation"]
		assert(data.has("held_damage"), "Missing ScheduledFrameStatus.held_damage")
		assert(data["held_damage"] == null or typeof(data["held_damage"]) == TYPE_FLOAT, "Invalid ScheduledFrameStatus.held_damage")
		out.held_damage = data["held_damage"]
		assert(data.has("fresh_tick91_damage"), "Missing ScheduledFrameStatus.fresh_tick91_damage")
		assert(data["fresh_tick91_damage"] == null or typeof(data["fresh_tick91_damage"]) == TYPE_FLOAT, "Invalid ScheduledFrameStatus.fresh_tick91_damage")
		out.fresh_tick91_damage = data["fresh_tick91_damage"]
		assert(data.has("rows"), "Missing ScheduledFrameStatus.rows")
		assert(typeof(data["rows"]) == TYPE_INT, "Invalid ScheduledFrameStatus.rows")
		out.rows = data["rows"]
		assert(data.has("window_frames"), "Missing ScheduledFrameStatus.window_frames")
		assert(typeof(data["window_frames"]) == TYPE_INT, "Invalid ScheduledFrameStatus.window_frames")
		out.window_frames = data["window_frames"]
		assert(data.has("renderer_generation"), "Missing ScheduledFrameStatus.renderer_generation")
		assert(typeof(data["renderer_generation"]) == TYPE_INT, "Invalid ScheduledFrameStatus.renderer_generation")
		out.renderer_generation = data["renderer_generation"]
		assert(data.has("observed_simulation_tick"), "Missing ScheduledFrameStatus.observed_simulation_tick")
		assert(typeof(data["observed_simulation_tick"]) == TYPE_INT, "Invalid ScheduledFrameStatus.observed_simulation_tick")
		out.observed_simulation_tick = data["observed_simulation_tick"]
		return out

	func to_wire() -> Dictionary:
		return {
			"simulation_tick": simulation_tick,
			"published_tick": published_tick,
			"generation": generation,
			"skipped_publications": skipped_publications,
			"published": published,
			"advances": advances,
			"restored": restored,
			"held_generation": held_generation,
			"held_damage": held_damage,
			"fresh_tick91_damage": fresh_tick91_damage,
			"rows": rows,
			"window_frames": window_frames,
			"renderer_generation": renderer_generation,
			"observed_simulation_tick": observed_simulation_tick,
		}

class ExternalStatus:
	var source_pid: int
	var source_generation: int
	var renderer_generation: int
	var published_tick: int
	var source_elapsed_us: int
	var skipped_generations: int
	var consumer_pid: int
	var ipc_sql_exact: bool

	static func from_wire(data: Dictionary) -> ExternalStatus:
		var out := ExternalStatus.new()
		assert(data.has("source_pid"), "Missing ExternalStatus.source_pid")
		assert(typeof(data["source_pid"]) == TYPE_INT, "Invalid ExternalStatus.source_pid")
		out.source_pid = data["source_pid"]
		assert(data.has("source_generation"), "Missing ExternalStatus.source_generation")
		assert(typeof(data["source_generation"]) == TYPE_INT, "Invalid ExternalStatus.source_generation")
		out.source_generation = data["source_generation"]
		assert(data.has("renderer_generation"), "Missing ExternalStatus.renderer_generation")
		assert(typeof(data["renderer_generation"]) == TYPE_INT, "Invalid ExternalStatus.renderer_generation")
		out.renderer_generation = data["renderer_generation"]
		assert(data.has("published_tick"), "Missing ExternalStatus.published_tick")
		assert(typeof(data["published_tick"]) == TYPE_INT, "Invalid ExternalStatus.published_tick")
		out.published_tick = data["published_tick"]
		assert(data.has("source_elapsed_us"), "Missing ExternalStatus.source_elapsed_us")
		assert(typeof(data["source_elapsed_us"]) == TYPE_INT, "Invalid ExternalStatus.source_elapsed_us")
		out.source_elapsed_us = data["source_elapsed_us"]
		assert(data.has("skipped_generations"), "Missing ExternalStatus.skipped_generations")
		assert(typeof(data["skipped_generations"]) == TYPE_INT, "Invalid ExternalStatus.skipped_generations")
		out.skipped_generations = data["skipped_generations"]
		assert(data.has("consumer_pid"), "Missing ExternalStatus.consumer_pid")
		assert(typeof(data["consumer_pid"]) == TYPE_INT, "Invalid ExternalStatus.consumer_pid")
		out.consumer_pid = data["consumer_pid"]
		assert(data.has("ipc_sql_exact"), "Missing ExternalStatus.ipc_sql_exact")
		assert(typeof(data["ipc_sql_exact"]) == TYPE_BOOL, "Invalid ExternalStatus.ipc_sql_exact")
		out.ipc_sql_exact = data["ipc_sql_exact"]
		return out

	func to_wire() -> Dictionary:
		return {
			"source_pid": source_pid,
			"source_generation": source_generation,
			"renderer_generation": renderer_generation,
			"published_tick": published_tick,
			"source_elapsed_us": source_elapsed_us,
			"skipped_generations": skipped_generations,
			"consumer_pid": consumer_pid,
			"ipc_sql_exact": ipc_sql_exact,
		}

class ControlledStatus:
	var simulation_tick: int
	var renderer_generation: int
	var input: ControlInput

	static func from_wire(data: Dictionary) -> ControlledStatus:
		var out := ControlledStatus.new()
		assert(data.has("simulation_tick"), "Missing ControlledStatus.simulation_tick")
		assert(typeof(data["simulation_tick"]) == TYPE_INT, "Invalid ControlledStatus.simulation_tick")
		out.simulation_tick = data["simulation_tick"]
		assert(data.has("renderer_generation"), "Missing ControlledStatus.renderer_generation")
		assert(typeof(data["renderer_generation"]) == TYPE_INT, "Invalid ControlledStatus.renderer_generation")
		out.renderer_generation = data["renderer_generation"]
		assert(data.has("input"), "Missing ControlledStatus.input")
		out.input = ControlInput.from_wire(data["input"])
		return out

	func to_wire() -> Dictionary:
		return {
			"simulation_tick": simulation_tick,
			"renderer_generation": renderer_generation,
			"input": input.to_wire(),
		}

class ControlInput:
	var buttons: int
	var axis: float

	static func from_wire(data: Dictionary) -> ControlInput:
		var out := ControlInput.new()
		assert(data.has("buttons"), "Missing ControlInput.buttons")
		assert(data["buttons"] >= 0, "Out of range ControlInput.buttons")
		assert(data["buttons"] <= 3, "Out of range ControlInput.buttons")
		assert(typeof(data["buttons"]) == TYPE_INT, "Invalid ControlInput.buttons")
		out.buttons = data["buttons"]
		assert(data.has("axis"), "Missing ControlInput.axis")
		assert(data["axis"] >= -1, "Out of range ControlInput.axis")
		assert(data["axis"] <= 1, "Out of range ControlInput.axis")
		assert(typeof(data["axis"]) == TYPE_FLOAT, "Invalid ControlInput.axis")
		out.axis = data["axis"]
		return out

	func to_wire() -> Dictionary:
		return {
			"buttons": buttons,
			"axis": axis,
		}

class BufferPolicy:
	var window_frames: int

	static func from_wire(data: Dictionary) -> BufferPolicy:
		var out := BufferPolicy.new()
		assert(data.has("window_frames"), "Missing BufferPolicy.window_frames")
		assert(typeof(data["window_frames"]) == TYPE_INT, "Invalid BufferPolicy.window_frames")
		out.window_frames = data["window_frames"]
		return out

	func to_wire() -> Dictionary:
		return {
			"window_frames": window_frames,
		}

class BufferInspection:
	var tick: int
	var pending: bool
	var remaining_frames: int
	var consumed: bool
	var expired: bool
	var cancelled: bool

	static func from_wire(data: Dictionary) -> BufferInspection:
		var out := BufferInspection.new()
		assert(data.has("tick"), "Missing BufferInspection.tick")
		assert(typeof(data["tick"]) == TYPE_INT, "Invalid BufferInspection.tick")
		out.tick = data["tick"]
		assert(data.has("pending"), "Missing BufferInspection.pending")
		assert(typeof(data["pending"]) == TYPE_BOOL, "Invalid BufferInspection.pending")
		out.pending = data["pending"]
		assert(data.has("remaining_frames"), "Missing BufferInspection.remaining_frames")
		assert(typeof(data["remaining_frames"]) == TYPE_INT, "Invalid BufferInspection.remaining_frames")
		out.remaining_frames = data["remaining_frames"]
		assert(data.has("consumed"), "Missing BufferInspection.consumed")
		assert(typeof(data["consumed"]) == TYPE_BOOL, "Invalid BufferInspection.consumed")
		out.consumed = data["consumed"]
		assert(data.has("expired"), "Missing BufferInspection.expired")
		assert(typeof(data["expired"]) == TYPE_BOOL, "Invalid BufferInspection.expired")
		out.expired = data["expired"]
		assert(data.has("cancelled"), "Missing BufferInspection.cancelled")
		assert(typeof(data["cancelled"]) == TYPE_BOOL, "Invalid BufferInspection.cancelled")
		out.cancelled = data["cancelled"]
		return out

	func to_wire() -> Dictionary:
		return {
			"tick": tick,
			"pending": pending,
			"remaining_frames": remaining_frames,
			"consumed": consumed,
			"expired": expired,
			"cancelled": cancelled,
		}

class FramePayload:
	var rows: PackedFloat64Array
	var vertices: PackedVector3Array
	var colors: PackedColorArray
	var status: Variant

	static func from_wire(data: Dictionary) -> FramePayload:
		var out := FramePayload.new()
		assert(data.has("rows"), "Missing FramePayload.rows")
		assert(data["rows"].size() <= 27648, "Oversized FramePayload.rows")
		assert(typeof(data["rows"]) == TYPE_PACKED_FLOAT64_ARRAY, "Invalid FramePayload.rows")
		out.rows = data["rows"]
		assert(data.has("vertices"), "Missing FramePayload.vertices")
		assert(data["vertices"].size() <= 100000, "Oversized FramePayload.vertices")
		assert(typeof(data["vertices"]) == TYPE_PACKED_VECTOR3_ARRAY, "Invalid FramePayload.vertices")
		out.vertices = data["vertices"]
		assert(data.has("colors"), "Missing FramePayload.colors")
		assert(data["colors"].size() <= 100000, "Oversized FramePayload.colors")
		assert(typeof(data["colors"]) == TYPE_PACKED_COLOR_ARRAY, "Invalid FramePayload.colors")
		out.colors = data["colors"]
		assert(data.has("status"), "Missing FramePayload.status")
		out.status = FrameStatus.from_wire(data["status"])
		return out

	func to_wire() -> Dictionary:
		return {
			"rows": rows,
			"vertices": vertices,
			"colors": colors,
			"status": status.to_wire(),
		}

class MeshReceipt:
	var generation: int
	var rows: PackedFloat64Array
	var vertices: PackedVector3Array

	static func from_wire(data: Dictionary) -> MeshReceipt:
		var out := MeshReceipt.new()
		assert(data.has("generation"), "Missing MeshReceipt.generation")
		assert(typeof(data["generation"]) == TYPE_INT, "Invalid MeshReceipt.generation")
		out.generation = data["generation"]
		assert(data.has("rows"), "Missing MeshReceipt.rows")
		assert(data["rows"].size() <= 27648, "Oversized MeshReceipt.rows")
		assert(typeof(data["rows"]) == TYPE_PACKED_FLOAT64_ARRAY, "Invalid MeshReceipt.rows")
		out.rows = data["rows"]
		assert(data.has("vertices"), "Missing MeshReceipt.vertices")
		assert(data["vertices"].size() <= 100000, "Oversized MeshReceipt.vertices")
		assert(typeof(data["vertices"]) == TYPE_PACKED_VECTOR3_ARRAY, "Invalid MeshReceipt.vertices")
		out.vertices = data["vertices"]
		return out

	func to_wire() -> Dictionary:
		return {
			"generation": generation,
			"rows": rows,
			"vertices": vertices,
		}

class FrameStatus:
	static func from_wire(data: Dictionary) -> Variant:
		if data.has("simulation_tick") and data.has("renderer_generation") and data.has("input"):
			return ControlledStatus.from_wire(data)
		if data.has("simulation_tick") and data.has("published_generation") and data.has("renderer_generation") and data.has("rows") and data.has("window_frames") and data.has("held_generation") and data.has("held_damage") and data.has("fresh_tick91_damage") and data.has("restored") and data.has("saved") and data.has("advances") and data.has("runtime_next_tick") and data.has("input_bits"):
			return FixtureStatus.from_wire(data)
		if data.has("simulation_tick") and data.has("published_tick") and data.has("generation") and data.has("skipped_publications") and data.has("published") and data.has("advances") and data.has("restored") and data.has("held_generation") and data.has("held_damage") and data.has("fresh_tick91_damage") and data.has("rows") and data.has("window_frames") and data.has("renderer_generation") and data.has("observed_simulation_tick"):
			return ScheduledFrameStatus.from_wire(data)
		if data.has("source_pid") and data.has("source_generation") and data.has("renderer_generation") and data.has("published_tick") and data.has("source_elapsed_us") and data.has("skipped_generations") and data.has("consumer_pid") and data.has("ipc_sql_exact"):
			return ExternalStatus.from_wire(data)
		assert(false, "Unknown status shape")
		return null

