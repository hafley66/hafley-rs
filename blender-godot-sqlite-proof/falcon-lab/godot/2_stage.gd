extends Node3D

const Rows = preload("res://1_rows_auto.gd")
const Payload = preload("res://1_payload_auto.gd")

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

func _ready():
	if not ClassDB.class_exists("FalconSql"):
		var loaded := GDExtensionManager.load_extension("res://0_falcon.gdextension")
		assert(loaded == GDExtensionManager.LOAD_STATUS_OK)
	extension = ClassDB.instantiate("FalconSql")
	assert(extension.proof_version() == "falcon-sql-gdext-1")
	external = "--external" in OS.get_cmdline_user_args()
	control_demo = "--control-demo" in OS.get_cmdline_user_args()
	if OS.has_feature("web"):
		control_demo = bool(JavaScriptBridge.eval("window.FALCON_DEMO === true"))
		web_inspect = bool(JavaScriptBridge.eval("new URL(location.href).searchParams.has('inspect')"))
	controlled = control_demo or "--control" in OS.get_cmdline_user_args() or OS.has_feature("web")
	incremental = "--incremental" in OS.get_cmdline_user_args()
	faults = "--faults" in OS.get_cmdline_user_args()
	scheduled = faults or "--scheduled" in OS.get_cmdline_user_args()
	if controlled:
		extension.start_controlled(control_demo)
	elif external:
		extension.start_external(OS.get_environment("FALCON_LIVE_PATH"), OS.get_environment("FALCON_LIVE_AUDIT"))
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
	var canvas := CanvasLayer.new()
	add_child(canvas)
	var font := SystemFont.new()
	font.font_names = PackedStringArray(["Menlo", "monospace"])
	for y in [16, 49, 76, 103, 130, 157, 438, 464, 491, 518]:
		var caption := Label.new()
		caption.position = Vector2(24, y)
		caption.add_theme_font_override("font", font)
		caption.add_theme_font_size_override("font_size", 20 if y == 16 else (12 if y == 518 else 16))
		caption.modulate = Color("65d9e6") if captions.size() % 2 else Color("e0e8f5")
		canvas.add_child(caption)
		captions.append(caption)
	captions[0].text = "FALCON -> RECYCLED SQLITE -> GDEXT -> GODOT"
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
	captions[5].text = "3D LINE MESH / SQL ROWS + MESH UPLOAD VERIFIED"
	captions[9].text = "0.5X + HOLDS / SCRIPTED TRAVEL / PM + MELEE KB + RAPIER"
	print("GDEXT_STAGE_READY runtime=", Engine.get_version_info().string)
	if OS.has_feature("web"):
		for i in [7, 8, 9]:
			captions[i].hide()
		var bar := HBoxContainer.new()
		bar.position = Vector2(24, 482)
		bar.size = Vector2(912, 44)
		canvas.add_child(bar)
		for title in ["Left", "Right", "Jump", "Fair", "Reset", "Proof"]:
			var button := Button.new()
			button.text = title
			button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			button.focus_mode = Control.FOCUS_NONE
			bar.add_child(button)
			match title:
				"Left":
					button.button_down.connect(func(): touch_left = true)
					button.button_up.connect(func(): touch_left = false)
				"Right":
					button.button_down.connect(func(): touch_right = true)
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
	var note_path := OS.get_environment("FALCON_LIVE_NOTE")
	if FileAccess.file_exists(note_path):
		captions[6].text = FileAccess.get_file_as_string(note_path)
	captions[9].text = "LIVE 40/41MS PEER CLOCKS / DT 1/60 / MOVIE OMITS STOPPED WALL TIME"
	RenderingServer.force_draw(false)
	if FileAccess.file_exists(OS.get_environment("FALCON_LIVE_STOP")):
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

func _physics_process(_delta):
	if not controlled or control_demo:
		return
	if Input.is_physical_key_pressed(KEY_ESCAPE):
		get_tree().quit(0)
		return
	var input := Payload.ControlInput.new()
	input.buttons = touch_buttons | int(Input.is_physical_key_pressed(KEY_SPACE)) | (int(Input.is_physical_key_pressed(KEY_J)) << 1)
	input.axis = float(touch_right or Input.is_physical_key_pressed(KEY_D) or Input.is_physical_key_pressed(KEY_RIGHT)) - float(touch_left or Input.is_physical_key_pressed(KEY_A) or Input.is_physical_key_pressed(KEY_LEFT))
	_control_step(input)

func _process_control_demo():
	if tick == extension.control_ticks() - 1 and remaining == 0:
		if finish_hold == 60:
			var verified: bool = extension.finish_controlled(OS.get_environment("FALCON_CONTROL_PROOF"))
			assert(verified)
			captions[6].text = "SNAPSHOT REPLAY: 120 STATES EXACT / SQL + MESH EXACT"
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
		JavaScriptBridge.eval("window.FALCON_STATUS = " + JSON.stringify(state.to_wire()))
		JavaScriptBridge.eval("window.FALCON_META = " + JSON.stringify(meta))
	captions[1].text = "TICK %03d / AXIS %+.1f / BUTTONS %d / SQL GEN %d" % [state.simulation_tick, state.input.axis, state.input.buttons, state.renderer_generation]
	captions[2].text = "%s POSE %02d / PLAYER Z %.1f Y %.1f" % [["IDLE", "JUMP", "FAIR"][int(meta.action)], int(meta.pose)+1, meta.root_z, meta.root_y]
	captions[3].text = "HITS %.0f / DAMAGE %.0f / BAG %s" % [meta.hits, meta.damage, ["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][int(target.phase)]]
	captions[4].text = "BAG Z %.1f Y %.1f / CONTACT %s / STUN %.0f" % [target.z, target.y, str(meta.contact != 0.0), target.stun]
	captions[5].text = "GENERATED INPUT + FRAME + RECEIPT / ROWS AND MESH EXACT"
	captions[6].text = "SCRIPTED INPUT REPLAY / SNAPSHOT CHECK PENDING" if control_demo else "LIVE KEYBOARD / LOCAL FIXED STEP / NO NETWORK PREDICTION"
	captions[7].text = "A/D OR ARROWS: MOVE / SPACE: JUMP / J: FAIR / ESC: EXIT"
	captions[8].text = "RUST PARRY CONTACT / RAPIER BAG / PM POSES"
	captions[9].text = "0.5X SCRIPTED DEMO / LAB MOVEMENT CURVE / SECOND FAIR MISSES" if control_demo else "60 HZ INPUT / LAB MOVEMENT CURVE / FACING RIGHT"

func _notification(what):
	if what == NOTIFICATION_APPLICATION_FOCUS_OUT:
		touch_left = false
		touch_right = false
		touch_buttons = 0
