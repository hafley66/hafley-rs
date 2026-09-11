extends Node3D

const Rows = preload("res://1_rows_auto.gd")
const Payload = preload("res://1_payload_auto.gd")
const InputAxis = preload("res://2_input.gd")

var extension
var mesh := ArrayMesh.new()
var captions: Array[Label] = []
var tick := -1
var remaining := 0
var video_frames := 0
var incremental := false
var scheduled := false
var displayed_tick := -1
var displayed_generation := 0
var finish_hold := 60
var last_video_time := 0
var faults := false
var external := false
var controlled := false
var control_demo := false
var touch_left := false
var touch_right := false
var touch_buttons := 0
var web_inspect := false
var injected := [false, false, false]
var fault_note := "FAULTS ARMED: MAIN / PROCESS / RENDER THREAD"
var direction := InputAxis.new()
const MOTION_PHASE_NAMES := ["IDLE", "WALK", "DASH", "RUN", "BRAKE", "TURN", "SQUAT", "CROUCH_ENTER", "CROUCH_HOLD", "CROUCH_EXIT", "LANDING", "JUMP", "FALL", "AIRJUMP"]
var motion_phase := -1
var observed_edges := {}
var last_control_tick := -1
var previous_motion_phase := -1
var motion_enter_tick := -1
var phase_graph: GraphEdit
var phase_nodes := {}
var newest_phase := -1
var previous_root_y := NAN

# Debug overlay stays inside the 960x540 base viewport. The stretched canvas can
# shrink debug text below physical readability on a portrait phone, so sizes are
# derived from the physical window: base size * scale = physical pixels.
const DEBUG_BASE := Vector2(960, 540)
const DEBUG_MIN_TEXT_PX := 14.0
const DEBUG_MIN_TOUCH_PX := 44.0
const DEBUG_GRAPH_ZOOM := 0.65
# Existing 12-phase order, grouped into four lifecycle bands: ground locomotion,
# crouch/jumpsquat, air, landing. Column index is the lifecycle band.
const PHASE_LIFECYCLE := [[0, 1, 2, 3, 4, 5], [6, 7, 8, 9], [11, 12, 13], [10]]
var canvas_layer: CanvasLayer
var debug_panel: ScrollContainer
var debug_rows: VBoxContainer
var touch_bar: HBoxContainer
var debug_scale := 1.0
var debug_geometry := {}

static func debug_scale_for(window_px: Vector2) -> float:
	if window_px.x <= 0.0 or window_px.y <= 0.0:
		return 1.0
	return minf(window_px.x / DEBUG_BASE.x, window_px.y / DEBUG_BASE.y)

# Smallest font that still renders at DEBUG_MIN_TEXT_PX physical pixels.
static func debug_font_for(base: int, scale: float) -> int:
	return int(ceil(maxf(float(base), DEBUG_MIN_TEXT_PX / maxf(scale, 0.0001))))

# Smallest touch target that still renders at DEBUG_MIN_TOUCH_PX physical pixels.
static func debug_touch_for(scale: float) -> float:
	return ceil(DEBUG_MIN_TOUCH_PX / maxf(scale, 0.0001))

static func phase_graph_cell(id: int) -> Vector2:
	for column in range(PHASE_LIFECYCLE.size()):
		var row: int = PHASE_LIFECYCLE[column].find(id)
		if row >= 0:
			return Vector2(12.0 + column * 190.0, 12.0 + row * 70.0)
	return Vector2(12.0, 12.0)

func _create_captions() -> void:
	var font := SystemFont.new()
	font.font_names = PackedStringArray(["Menlo", "monospace"])
	for y in [16, 49, 76, 103, 130, 157, 438, 464, 491, 518]:
		var caption := Label.new()
		caption.position = Vector2(24, y)
		caption.add_theme_font_override("font", font)
		caption.add_theme_font_size_override("font_size", 20 if y == 16 else (12 if y == 518 else 16))
		caption.modulate = Color("65d9e6") if captions.size() % 2 else Color("e0e8f5")
		canvas_layer.add_child(caption)
		captions.append(caption)

static func phase_label(phase: int, previous: int, entry_tick: int) -> String:
	var previous_name: String = "NONE" if previous < 0 else MOTION_PHASE_NAMES[previous]
	return "PHASE %s / PREV %s / T%d / OBSERVED GRAPH" % [MOTION_PHASE_NAMES[phase], previous_name, entry_tick]

static func edges_label(edges: Array, newest: int) -> String:
	var list: String = "NONE" if edges.is_empty() else ", ".join(PackedStringArray(edges))
	var note: String = "" if newest < 0 else " / NEW %s" % MOTION_PHASE_NAMES[newest]
	return "OBSERVED EDGES: %s%s" % [list, note]

static func velocity_label(speed: float, vy: float, age: int) -> String:
	return "VEL X %+.3f / VY %+.3f / PHASE AGE %d TICKS" % [speed, vy, age]

func _ready():
	if not ClassDB.class_exists("PigeonSql"):
		var loaded := GDExtensionManager.load_extension("res://0_pigeon.gdextension")
		assert(loaded == GDExtensionManager.LOAD_STATUS_OK)
	extension = ClassDB.instantiate("PigeonSql")
	assert(extension.proof_version() == "pigeon-sql-gdext-1")
	external = "--external" in OS.get_cmdline_user_args()
	control_demo = "--control-demo" in OS.get_cmdline_user_args()
	if OS.has_feature("web"):
		control_demo = bool(JavaScriptBridge.eval("window.PIGEON_DEMO === true"))
		web_inspect = bool(JavaScriptBridge.eval("new URL(location.href).searchParams.has('inspect')"))
	controlled = control_demo or "--control" in OS.get_cmdline_user_args() or OS.has_feature("web")
	incremental = "--incremental" in OS.get_cmdline_user_args()
	faults = "--faults" in OS.get_cmdline_user_args()
	scheduled = faults or "--scheduled" in OS.get_cmdline_user_args()
	if controlled:
		extension.start_controlled(control_demo)
	elif external:
		extension.start_external(OS.get_environment("PIGEON_LIVE_PATH"), OS.get_environment("PIGEON_LIVE_AUDIT"))
	elif faults:
		extension.start_faults()
	elif scheduled:
		extension.start_scheduled()
	elif incremental:
		extension.start_incremental()
	else:
		extension.start()
	var instance := MeshInstance3D.new()
	instance.mesh = mesh
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.vertex_color_use_as_albedo = true
	instance.material_override = material
	add_child(instance)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 83.076923
	camera.position = Vector3(82, 38, 150)
	camera.current = true
	add_child(camera)
	camera.look_at(Vector3(40, 23, 0))
	canvas_layer = CanvasLayer.new()
	add_child(canvas_layer)
	_create_captions()
	captions[0].text = "PIGEON -> RECYCLED SQLITE -> GDEXT -> GODOT"
	if incremental:
		captions[0].text = "INPUT -> RUST TICK -> SQLITE -> GODOT"
	if scheduled:
		captions[0].text = "RUST CLOCK -> SQLITE / GODOT CONSUMER PAUSE"
	if faults:
		captions[0].text = "RUST WORKER / MAIN + PROCESS + RENDER STALLS"
		print("MAIN_THREAD_ID ", OS.get_thread_caller_id())
	if external:
		captions[0].text = "LIVE EXTERNAL PEER -> SNAPSHOT -> LOCAL SQLITE -> GODOT"
	if controlled:
		captions[0].text = "PLAYER INPUT -> RUST -> SQLITE -> TYPED GODOT"
		if not control_demo:
			phase_graph = GraphEdit.new()
			phase_graph.position = Vector2(560, 55)
			phase_graph.size = Vector2(376, 210)
			phase_graph.show_menu = false
			phase_graph.minimap_enabled = false
			phase_graph.show_grid = false
			phase_graph.zoom = 0.65
			canvas_layer.add_child(phase_graph)
			captions[5].add_theme_font_size_override("font_size", 12)
	captions[5].text = "3D LINE MESH / SQL ROWS + MESH UPLOAD VERIFIED"
	captions[9].text = "0.5X + HOLDS / SCRIPTED TRAVEL / PM + MELEE KB + RAPIER"
	print("GDEXT_STAGE_READY runtime=", Engine.get_version_info().string)
	if OS.has_feature("web"):
		for i in [7, 8, 9]:
			captions[i].hide()
		if not control_demo:
			captions[8].position = Vector2(24, 180)
			captions[8].add_theme_font_size_override("font_size", 13)
			captions[8].show()
		touch_bar = HBoxContainer.new()
		touch_bar.position = Vector2(24, 482)
		touch_bar.size = Vector2(912, 44)
		canvas_layer.add_child(touch_bar)
		for title in ["Left", "Right", "Jump", "Fair", "Reset", "Proof"]:
			var button := Button.new()
			button.text = title
			button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			button.focus_mode = Control.FOCUS_NONE
			touch_bar.add_child(button)
			match title:
				"Left":
					button.button_down.connect(func(): touch_left = true; direction.press(-1))
					button.button_up.connect(func(): touch_left = false)
				"Right":
					button.button_down.connect(func(): touch_right = true; direction.press(1))
					button.button_up.connect(func(): touch_right = false)
				"Jump":
					button.button_down.connect(func(): touch_buttons |= 1)
					button.button_up.connect(func(): touch_buttons &= ~1)
				"Fair":
					button.button_down.connect(func(): touch_buttons |= 2)
					button.button_up.connect(func(): touch_buttons &= ~2)
				"Reset":
					button.pressed.connect(func(): JavaScriptBridge.eval("location.search = ''"))
				"Proof":
					button.pressed.connect(func(): JavaScriptBridge.eval("location.search = '?demo=1'"))
	if controlled and not control_demo:
		_build_debug_panel()
		_layout_debug_overlay(Vector2(DisplayServer.window_get_size()))
		get_viewport().size_changed.connect(_on_viewport_size_changed)

func _on_viewport_size_changed() -> void:
	_layout_debug_overlay(Vector2(DisplayServer.window_get_size()))

# One scroll panel holds the telemetry labels and the observed graph, so a narrow
# stretched viewport scrolls instead of shrinking every label below readability.
func _build_debug_panel() -> void:
	if debug_panel != null or captions.is_empty():
		return
	debug_panel = ScrollContainer.new()
	debug_panel.name = "DebugPanel"
	debug_panel.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	debug_rows = VBoxContainer.new()
	debug_rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	debug_panel.add_child(debug_rows)
	for caption in captions:
		caption.get_parent().remove_child(caption)
		caption.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		caption.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		debug_rows.add_child(caption)
	if phase_graph != null:
		phase_graph.get_parent().remove_child(phase_graph)
		debug_rows.add_child(phase_graph)
	canvas_layer.add_child(debug_panel)

func _layout_debug_overlay(window_px: Vector2) -> void:
	debug_scale = debug_scale_for(window_px)
	var narrow: bool = window_px.x < 700.0
	if not captions.is_empty():
		var body := debug_font_for(16, debug_scale)
		for index in range(captions.size()):
			captions[index].add_theme_font_size_override("font_size", body)
		captions[0].add_theme_font_size_override("font_size", debug_font_for(20, debug_scale))
		captions[9].add_theme_font_size_override("font_size", debug_font_for(12, debug_scale))
	if debug_panel != null:
		var touch := debug_touch_for(debug_scale) if touch_bar != null else 0.0
		debug_panel.position = Vector2(8, 8)
		debug_panel.size = Vector2(
			DEBUG_BASE.x - 16.0 if narrow else 520.0,
			DEBUG_BASE.y - touch - 16.0)
		debug_rows.add_theme_constant_override("separation", int(maxf(4.0, debug_scale * 10.0)))
	if touch_bar != null:
		var touch_h := debug_touch_for(debug_scale)
		touch_bar.position = Vector2(8, DEBUG_BASE.y - touch_h)
		touch_bar.size = Vector2(DEBUG_BASE.x - 16, touch_h)
		for button in touch_bar.get_children():
			button.custom_minimum_size = Vector2(0, touch_h)
	if phase_graph != null:
		phase_graph.zoom = DEBUG_GRAPH_ZOOM
		phase_graph.custom_minimum_size = Vector2(760, 560) if narrow else Vector2(420, 560)
		_apply_graph_layout()
	_remember_debug_geometry()

# Deterministic snapshot the headless geometry test reads back.
func _remember_debug_geometry() -> void:
	debug_geometry = {
		"scale": debug_scale,
		"body_font": debug_font_for(16, debug_scale),
		"body_physical": debug_font_for(16, debug_scale) * debug_scale,
		"touch_logical": debug_touch_for(debug_scale),
		"touch_physical": debug_touch_for(debug_scale) * debug_scale,
		"panel": Vector2.ZERO if debug_panel == null else debug_panel.size,
		"cells": phase_graph_cells(),
	}

static func phase_graph_cells() -> Dictionary:
	var cells := {}
	for id in range(MOTION_PHASE_NAMES.size()):
		cells[id] = phase_graph_cell(id)
	return cells

func _upload_and_acknowledge(frame: Payload.FramePayload, generation: int) -> void:
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = frame.vertices
	arrays[Mesh.ARRAY_COLOR] = frame.colors
	mesh.clear_surfaces()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, arrays)
	var uploaded := mesh.surface_get_arrays(0)
	var receipt := Payload.MeshReceipt.new()
	receipt.generation = generation
	receipt.rows = frame.rows
	receipt.vertices = uploaded[Mesh.ARRAY_VERTEX]
	var acknowledged: bool = extension.acknowledge(receipt.to_wire())
	assert(acknowledged)

func _process(_delta):
	if controlled:
		if control_demo:
			_process_control_demo()
		return
	if external:
		_process_external()
		return
	if scheduled:
		_process_scheduled()
		return
	if remaining == 0:
		if tick == 179:
			var finished: bool = extension.finish()
			assert(finished)
			assert(video_frames == 824)
			print("GODOT_CAPTURE_OK frames=", video_frames)
			get_tree().quit(0)
			set_process(false)
			return
		tick += 1
		var bits := 1 if tick == 60 else (2 if tick == 78 else 0)
		var frame := Payload.FramePayload.from_wire(extension.advance(bits) if incremental else extension.next_frame())
		var state = frame.status
		var rows: PackedFloat64Array = frame.rows
		var meta := Rows.frame_values(rows)
		var target := Rows.target_values(rows)
		assert(not meta.is_empty() and not target.is_empty())
		assert(state.simulation_tick == tick)
		assert(state.published_generation == state.renderer_generation)
		_upload_and_acknowledge(frame, int(state.renderer_generation))
		captions[1].text = "SIM %03d / PUBLISHED %03d / RENDERER %03d" % [tick, state.published_generation, state.renderer_generation]
		captions[2].text = "%s POSE %02d / INPUT %s" % [["IDLE", "JUMP", "FAIR"][int(meta.action)], int(meta.pose) + 1, "PREDICTED" if meta.predicted != 0 else "CONFIRMED"]
		captions[3].text = "BAG %s / %.0f%% / STUN %.0f" % [["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][int(target.phase)], meta.damage, target.stun]
		captions[4].text = "SQL WINDOW %d/32 / ROWS %d/1024 / SLOTS 3" % [state.window_frames, state.rows]
		captions[6].text = "RESTORE %d / ATOMIC CORRECTION: 20 FRAMES" % state.restored[0] if not state.restored.is_empty() else "PUBLISH COMPLETE WINDOW / SLOT POINTERS STABLE"
		captions[7].text = "HELD SQL CURSOR: GEN %d / TICK 91 / DAMAGE 0" % state.held_generation if state.held_generation != null else ("HELD CURSOR RELEASED / SLOT REUSABLE" if tick == 101 else "HELD SQL CURSOR: NONE")
		captions[8].text = "FRESH SQL QUERY: TICK 91 DAMAGE %.0f" % state.fresh_tick91_damage if state.fresh_tick91_damage != null else "LATEST GENERATION / BOUNDED STORAGE"
		if incremental:
			assert(state.runtime_next_tick == tick + 1)
			captions[5].text = "EXECUTED NOW: %d / ADVANCES %d / INPUT SENT %d" % [state.runtime_next_tick, state.advances, bits]
			captions[6].text = "SAVES %d LAST %s / LOAD %s / NEXT %d" % [state.saved.size(), str(state.saved.back()) if not state.saved.is_empty() else "NONE", str(state.restored), state.runtime_next_tick]
		remaining = 60 if tick in [60, 78, 91, 97, 101, 117, 126, 179] else 2
	remaining -= 1
	video_frames += 1

func _process_external():
	var now := Time.get_ticks_usec()
	if last_video_time != 0 and now - last_video_time < 16667:
		OS.delay_usec(16667 - (now - last_video_time))
	last_video_time = Time.get_ticks_usec()
	var wire: Dictionary = extension.poll_external()
	if not wire.is_empty():
		var frame := Payload.FramePayload.from_wire(wire)
		var state = frame.status
		var rows: PackedFloat64Array = frame.rows
		var meta := Rows.frame_values(rows)
		var target := Rows.target_values(rows)
		assert(not meta.is_empty() and not target.is_empty())
		_upload_and_acknowledge(frame, int(state.renderer_generation))
		displayed_tick = int(state.published_tick)
		captions[1].text = "PEER PID %d / GODOT PID %d / TICK %03d" % [state.source_pid, OS.get_process_id(), displayed_tick]
		captions[2].text = "%s POSE %02d / INPUT %d %s / CONFIRMED %.0f" % [["IDLE", "JUMP", "FAIR"][int(meta.action)], int(meta.pose)+1, int(meta.input), "PREDICTED" if meta.predicted != 0 else "KNOWN", meta.confirmed]
		captions[3].text = "BAG %s / DAMAGE %.0f / STUN %.0f" % [["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][int(target.phase)], meta.damage, target.stun]
		captions[4].text = "SOURCE GEN %d / LOCAL SQL GEN %d / SKIPPED %d" % [state.source_generation, state.renderer_generation, state.skipped_generations]
		captions[5].text = "RESTORE %.0f / REPLAY %.0f / SOURCE WALL %.3fS" % [meta.restored, meta.advances-1, state.source_elapsed_us / 1000000.0]
		captions[7].text = "IPC ROWS -> SQLITE -> ARRAYMESH: EXACT / ROWS %d" % (rows.size()/Rows.STRIDE)
		captions[8].text = "RENDERER HAS NO SIMULATION WORKER / LATEST-ONLY FEED"
		print("LIVE_ACK ", displayed_tick, " ", int(state.source_generation))
	elif displayed_tick == -1:
		captions[1].text = "WAITING FOR EXTERNAL PEER SNAPSHOT"
	var note_path := OS.get_environment("PIGEON_LIVE_NOTE")
	if FileAccess.file_exists(note_path):
		captions[6].text = FileAccess.get_file_as_string(note_path)
	captions[9].text = "LIVE 40/41MS PEER CLOCKS / DT 1/60 / MOVIE OMITS STOPPED WALL TIME"
	RenderingServer.force_draw(false)
	if FileAccess.file_exists(OS.get_environment("PIGEON_LIVE_STOP")):
		print("LIVE_EXIT requested tick=", displayed_tick)
		get_tree().quit(0)
		set_process(false)
	if displayed_tick == 199:
		finish_hold -= 1
		if finish_hold == 0:
			print("LIVE_EXIT final tick=199")
			get_tree().quit(0)
			set_process(false)

func _process_scheduled():
	# Pace capture only. The Rust worker has its own Instant-based clock.
	var now := Time.get_ticks_usec()
	if last_video_time != 0 and now - last_video_time < 16667:
		OS.delay_usec(16667 - (now - last_video_time))
	last_video_time = Time.get_ticks_usec()
	var observation: Dictionary = extension.poll_scheduled(true)
	if not observation.has("current"):
		captions[1].text = "LOADING FIXTURE / WORKER HAS NOT EXECUTED A TICK"
		return
	var current := Payload.ScheduledStatus.from_wire(observation.current)
	var paused: bool = not faults and current.simulation_tick >= 92 and current.simulation_tick < 106
	if observation.has("frame"):
		var frame := Payload.FramePayload.from_wire(observation.frame)
		var state = frame.status
		var rows: PackedFloat64Array = frame.rows
		var meta := Rows.frame_values(rows)
		var target := Rows.target_values(rows)
		assert(not meta.is_empty() and not target.is_empty())
		_upload_and_acknowledge(frame, int(state.renderer_generation))
		displayed_tick = int(state.published_tick)
		displayed_generation = int(state.renderer_generation)
		captions[2].text = "DISPLAY: %s POSE %02d / INPUT %s" % [["IDLE", "JUMP", "FAIR"][int(meta.action)], int(meta.pose) + 1, "PREDICTED" if meta.predicted != 0 else "CONFIRMED"]
		captions[3].text = "DISPLAY BAG: %s / %.0f%% / STUN %.0f" % [["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][int(target.phase)], meta.damage, target.stun]
		if displayed_tick >= 106:
			assert(displayed_tick == int(current.simulation_tick))
	captions[1].text = "SIM %03d / DISPLAY %03d / LAG %02d / GEN %03d" % [current.simulation_tick, displayed_tick, int(current.simulation_tick) - displayed_tick, displayed_generation]
	captions[4].text = "PUBLISHED TICK %03d GEN %03d / SKIPPED %d / ROWS %d" % [current.published_tick, current.generation, current.skipped_publications, current.rows]
	captions[5].text = "CONSUMPTION PAUSED / WORKER CONTINUES" if paused else "CONSUMING LATEST SQL GENERATION"
	captions[6].text = "SIM ADVANCES %d / RESTORE %s / FIXED DT 1/60" % [current.advances, str(current.restored)]
	captions[7].text = "HELD SQL GEN %s DAMAGE %s / FRESH TICK91 %s" % [str(current.held_generation), str(current.held_damage), str(current.fresh_tick91_damage)]
	captions[8].text = "ALL SLOTS PINNED: SKIP PUBLICATION, KEEP STEPPING" if not current.published else "LATEST WINDOW PUBLISHED / 3 RECYCLED SLOTS"
	captions[9].text = "25 SIM TICKS/SEC CAPTURE / SCRIPTED INPUT / INDEPENDENT WORKER CLOCK"
	if faults:
		captions[5].text = fault_note
		captions[8].text = "SAME PROCESS / FIXED-STEP CATCH-UP / GOLDEN STATES CHECKED"
	RenderingServer.force_draw(false)
	video_frames += 1
	if faults:
		_inject_faults(current)
	if displayed_tick == 179:
		finish_hold -= 1
		if finish_hold == 0:
			var finished: bool = extension.finish_scheduled()
			assert(finished)
			print("SCHEDULE_CAPTURE_OK observed_video_frames=", video_frames)
			get_tree().quit(0)
			set_process(false)

func _inject_faults(current: Payload.ScheduledStatus):
	if current.simulation_tick >= 60 and not injected[0]:
		injected[0] = true
		print("FAULT_BEGIN main")
		OS.delay_msec(800)
		print("FAULT_END main")
		var after := Payload.ScheduledStatus.from_wire(extension.poll_scheduled(false).current)
		fault_note = "MAIN THREAD: 800 MS / WORKER ADVANCED %d TICKS" % (int(after.simulation_tick) - int(current.simulation_tick))
	elif current.simulation_tick >= 90 and not injected[1]:
		injected[1] = true
		print("PROCESS_STOP_READY")
		fault_note = "PROCESS STOP REQUESTED / EXTERNAL SIGSTOP + SIGCONT"
	elif current.simulation_tick >= 130 and not injected[2]:
		injected[2] = true
		RenderingServer.call_on_render_thread(_render_stall)
		RenderingServer.force_sync()
		var after := Payload.ScheduledStatus.from_wire(extension.poll_scheduled(false).current)
		fault_note = "RENDER THREAD: 800 MS / WORKER ADVANCED %d TICKS" % (int(after.simulation_tick) - int(current.simulation_tick))

static func _render_stall():
	print("RENDER_THREAD_ID ", OS.get_thread_caller_id())
	print("FAULT_BEGIN render")
	OS.delay_msec(800)
	print("FAULT_END render")

func _input(event):
	# Key order is the only tie-break a digital stick has. `_physics_process`
	# still polls held state; this records which opposing direction arrived
	# last so a reverse does not need the old key released first.
	if event is InputEventKey and event.pressed and not event.echo:
		match event.physical_keycode:
			KEY_A, KEY_LEFT:
				direction.press(-1)
			KEY_D, KEY_RIGHT:
				direction.press(1)

func _physics_process(_delta):
	if not controlled or control_demo:
		return
	if Input.is_physical_key_pressed(KEY_ESCAPE):
		get_tree().quit(0)
		return
	var input := Payload.ControlInput.new()
	input.buttons = touch_buttons | int(Input.is_physical_key_pressed(KEY_SPACE)) | (int(Input.is_physical_key_pressed(KEY_J)) << 1)
	input.buttons |= int(Input.is_physical_key_pressed(KEY_S) or Input.is_physical_key_pressed(KEY_DOWN)) << 2
	input.axis = direction.axis(
		touch_left or Input.is_physical_key_pressed(KEY_A) or Input.is_physical_key_pressed(KEY_LEFT),
		touch_right or Input.is_physical_key_pressed(KEY_D) or Input.is_physical_key_pressed(KEY_RIGHT))
	if Input.is_physical_key_pressed(KEY_SHIFT):
		input.axis *= 0.4
	var pads := Input.get_connected_joypads()
	if not pads.is_empty():
		var pad: int = pads[0]
		var stick := Input.get_joy_axis(pad, JOY_AXIS_LEFT_X)
		if absf(stick) > 0.2:
			input.axis = stick
		input.buttons |= int(Input.is_joy_button_pressed(pad, JOY_BUTTON_A) or Input.is_joy_button_pressed(pad, JOY_BUTTON_Y))
		input.buttons |= int(Input.is_joy_button_pressed(pad, JOY_BUTTON_X)) << 1
		input.buttons |= int(Input.get_joy_axis(pad, JOY_AXIS_LEFT_Y) > 0.65) << 2
	_control_step(input)

func _process_control_demo():
	if tick == extension.control_ticks() - 1 and remaining == 0:
		if finish_hold == 60:
			var proof_path := OS.get_environment("PIGEON_CONTROL_PROOF")
			if proof_path.is_empty():
				proof_path = "user://pigeon-control-proof.json"
			var verified: bool = extension.finish_controlled(proof_path)
			assert(verified)
			captions[6].text = "0.5X DEMO / SNAPSHOT REPLAY: 120 STATES EXACT / SQL + MESH EXACT"
		finish_hold -= 1
		if finish_hold == 0:
			print("CONTROL_CAPTURE_OK")
			if not OS.has_feature("web"):
				get_tree().quit(0)
			set_process(false)
		return
	if remaining == 0:
		tick += 1
		_control_step(Payload.ControlInput.from_wire(extension.control_demo_input(tick)))
		remaining = 2
	remaining -= 1

func _control_step(input: Payload.ControlInput):
	var frame := Payload.FramePayload.from_wire(extension.advance_controlled(input.to_wire()))
	var state = frame.status
	var meta := Rows.frame_values(frame.rows)
	var target := Rows.target_values(frame.rows)
	_upload_and_acknowledge(frame, state.renderer_generation)
	if web_inspect:
		JavaScriptBridge.eval("window.PIGEON_STATUS = " + JSON.stringify(state.to_wire()))
		JavaScriptBridge.eval("window.PIGEON_META = " + JSON.stringify(meta))
	captions[1].text = "TICK %03d / AXIS %+.1f / BUTTONS %d / SQL GEN %d" % [state.simulation_tick, state.input.axis, state.input.buttons, state.renderer_generation]
	captions[2].text = "%s POSE %02d / PLAYER Z %.1f Y %.1f" % [Payload.CONTROL_ACTION_LABELS.split("|")[int(meta.action)], int(meta.pose)+1, meta.root_z, meta.root_y]
	if not control_demo:
		var camera := get_viewport().get_camera_3d()
		camera.position = Vector3(meta.root_z + 42, 38, 150)
		camera.look_at(Vector3(meta.root_z, 23, 0))
	captions[3].text = "HITS %.0f / DAMAGE %.0f / BAG %s" % [meta.hits, meta.damage, ["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][int(target.phase)]]
	captions[4].text = "BAG Z %.1f Y %.1f / CONTACT %s / STUN %.0f" % [target.z, target.y, str(meta.contact != 0.0), target.stun]
	captions[5].text = "PM POSES + ATTRIBUTES / SHARED RUST LOCOMOTION" if not control_demo else "HISTORICAL KNEE REGRESSION / LAB MOVEMENT"
	captions[6].text = "0.5X SCRIPTED REPLAY / SNAPSHOT CHECK PENDING" if control_demo else "LIVE 60 HZ / SPEED %+.3f UNITS/TICK / FACING %s / LOCAL" % [meta.speed, "LEFT" if meta.facing < 0 else "RIGHT"]
	captions[7].text = "A/D: DASH / SHIFT: WALK / SPACE: JUMP / S: DOWN / J: FAIR"
	captions[8].text = "RUST PARRY CONTACT / RAPIER BAG / PM POSES"
	captions[9].text = "0.5X DEMO / LANDING RECOVERY / THIRD JUMP" if control_demo else "60 HZ / SPEED %+.3f UNITS/TICK / FACING %s" % [meta.speed, "LEFT" if meta.facing < 0 else "RIGHT"]
	# Actual live locomotion phase, not the animation action/pose above.
	# Previous phase and the transition tick are observed from the presented
	# stream; the edge list is observed only, never a complete legal-edge graph.
	if not control_demo:
		_observe_phase(int(state.simulation_tick), meta.phase, meta.phase_ticks)
		var vy: float = float(meta.root_y) - previous_root_y if is_finite(previous_root_y) else 0.0
		previous_root_y = float(meta.root_y)
		var age := 0 if motion_enter_tick < 0 else int(state.simulation_tick) - motion_enter_tick + 1
		captions[5].text = velocity_label(float(meta.speed), vy, age)
		if motion_phase >= 0:
			captions[8].text = phase_label(motion_phase, previous_motion_phase, motion_enter_tick)
			captions[9].text = edges_label(observed_edges.keys(), newest_phase)
		else:
			captions[8].text = "LIVE PHASE NONE / NO MOVEMENT STATE"
			captions[9].text = "OBSERVED EDGES (not a legal-edge graph): NONE"
		if web_inspect:
			JavaScriptBridge.eval("window.PIGEON_PHASE_DEBUG = " + JSON.stringify({"active": motion_phase, "previous": previous_motion_phase, "entry_tick": motion_enter_tick, "edges": observed_edges.keys(), "nodes": phase_nodes.keys()}))

func _reset_phase_view():
	motion_phase = -1
	previous_motion_phase = -1
	motion_enter_tick = -1
	newest_phase = -1
	observed_edges.clear()
	previous_root_y = NAN
	if phase_graph != null:
		phase_graph.clear_connections()
		for node in phase_nodes.values():
			phase_graph.remove_child(node)
			node.queue_free()
	phase_nodes.clear()

# Observer lifetime follows monotonically presented ticks, separate from Redux.
func _observe_phase(at_tick: int, code: float, age: float):
	if at_tick == last_control_tick and code == float(motion_phase):
		return
	if at_tick <= last_control_tick:
		_reset_phase_view()
	last_control_tick = at_tick
	if not is_finite(code) or code != floor(code) or code < 0 or code >= MOTION_PHASE_NAMES.size():
		_reset_phase_view()
		return
	var now := int(code)
	var entry := at_tick - int(age) + 1 if is_finite(age) and age >= 1 else at_tick
	# A restarted phase clock exposes a same-phase re-entry, including dash dance.
	# Skipped presentation ticks can hide intermediate transitions; this remains
	# an observed graph of published samples rather than a complete event log.
	if now == motion_phase and (entry <= motion_enter_tick or not is_finite(age) or age < 1):
		return
	previous_motion_phase = motion_phase
	motion_phase = now
	motion_enter_tick = entry
	if phase_graph != null and not phase_nodes.has(now):
		var node := GraphNode.new()
		node.name = MOTION_PHASE_NAMES[now]
		node.title = MOTION_PHASE_NAMES[now]
		node.position_offset = phase_graph_cell(now)
		var label := Label.new()
		label.text = "observed"
		node.add_child(label)
		node.set_slot(0, true, 0, Color("65d9e6"), true, 0, Color("65d9e6"))
		phase_graph.add_child(node)
		phase_nodes[now] = node
	if previous_motion_phase >= 0:
		newest_phase = now
		var edge := "%s->%s" % [MOTION_PHASE_NAMES[previous_motion_phase], MOTION_PHASE_NAMES[now]]
		observed_edges[edge] = observed_edges.get(edge, 0) + 1
		if phase_graph != null:
			phase_graph.connect_node(MOTION_PHASE_NAMES[previous_motion_phase], 0, MOTION_PHASE_NAMES[now], 0)
	if phase_nodes.has(now):
		var label: Label = phase_nodes[now].get_child(0)
		label.text = "entered T%d" % motion_enter_tick
		if now == previous_motion_phase:
			label.text += " / re-entry"
	_refresh_phase_titles()
	_apply_graph_layout()

# Non-color markers: the active node is prefixed, the newest transition gets a
# trailing arrow, so the two states are distinguishable without hue alone.
func _refresh_phase_titles() -> void:
	for id in phase_nodes:
		var title: String = MOTION_PHASE_NAMES[id]
		if id == motion_phase:
			title = "▶ " + title
		if id == newest_phase:
			title = title + " ←"
		phase_nodes[id].title = title
		phase_nodes[id].self_modulate = Color("65efb0") if id == motion_phase else Color("8793a8")

func _apply_graph_layout() -> void:
	if phase_graph == null:
		return
	var node_font := debug_font_for(14, debug_scale * DEBUG_GRAPH_ZOOM)
	for id in phase_nodes:
		var node: GraphNode = phase_nodes[id]
		node.position_offset = phase_graph_cell(id)
		node.custom_minimum_size = Vector2(maxf(150.0, node_font * 6.0), maxf(60.0, node_font * 2.2))
		node.add_theme_font_size_override("title_font_size", node_font)
		if node.get_child_count() > 0 and node.get_child(0) is Label:
			(node.get_child(0) as Label).add_theme_font_size_override("font_size", node_font)

func _notification(what):
	if what == NOTIFICATION_APPLICATION_FOCUS_OUT or what == NOTIFICATION_WM_WINDOW_FOCUS_OUT:
		touch_left = false
		touch_right = false
		touch_buttons = 0
		direction.reset()
