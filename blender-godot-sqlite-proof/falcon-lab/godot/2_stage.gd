extends Node3D

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
var injected := [false, false, false]
var fault_note := "FAULTS ARMED: MAIN / PROCESS / RENDER THREAD"

func _ready():
	if not ClassDB.class_exists("FalconSql"):
		assert(GDExtensionManager.load_extension("res://0_falcon.gdextension") == GDExtensionManager.LOAD_STATUS_OK)
	extension = ClassDB.instantiate("FalconSql")
	assert(extension.proof_version() == "falcon-sql-gdext-1")
	external = "--external" in OS.get_cmdline_user_args()
	incremental = "--incremental" in OS.get_cmdline_user_args()
	faults = "--faults" in OS.get_cmdline_user_args()
	scheduled = faults or "--scheduled" in OS.get_cmdline_user_args()
	if external:
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
	captions[5].text = "3D LINE MESH / SQL ROWS + MESH UPLOAD VERIFIED"
	captions[9].text = "0.5X + HOLDS / SCRIPTED TRAVEL / PM + MELEE KB + RAPIER"
	print("GDEXT_STAGE_READY runtime=", Engine.get_version_info().string)

func _process(_delta):
	if external:
		_process_external()
		return
	if scheduled:
		_process_scheduled()
		return
	if remaining == 0:
		if tick == 179:
			assert(extension.finish())
			assert(video_frames == 824)
			print("GODOT_CAPTURE_OK frames=", video_frames)
			get_tree().quit(0)
			set_process(false)
			return
		tick += 1
		var bits := 1 if tick == 60 else (2 if tick == 78 else 0)
		var frame: Dictionary = extension.advance(bits) if incremental else extension.next_frame()
		var state: Dictionary = JSON.parse_string(frame.status)
		var rows: PackedFloat64Array = frame.rows
		assert(state.simulation_tick == tick)
		assert(state.published_generation == state.renderer_generation)
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = frame.vertices
		arrays[Mesh.ARRAY_COLOR] = frame.colors
		mesh.clear_surfaces()
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, arrays)
		var uploaded := mesh.surface_get_arrays(0)
		assert(extension.acknowledge(int(state.renderer_generation), rows, uploaded[Mesh.ARRAY_VERTEX]))
		captions[1].text = "SIM %03d / PUBLISHED %03d / RENDERER %03d" % [tick, state.published_generation, state.renderer_generation]
		captions[2].text = "%s POSE %02d / INPUT %s" % [["IDLE", "JUMP", "FAIR"][int(rows[3])], int(rows[4]) + 1, "PREDICTED" if rows[12] != 0 else "CONFIRMED"]
		captions[3].text = "BAG %s / %.0f%% / STUN %.0f" % [["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][int(rows[37])], rows[8], rows[36]]
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
	var frame: Dictionary = extension.poll_external()
	if not frame.is_empty():
		var state: Dictionary = JSON.parse_string(frame.status)
		var rows: PackedFloat64Array = frame.rows
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = frame.vertices
		arrays[Mesh.ARRAY_COLOR] = frame.colors
		mesh.clear_surfaces()
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, arrays)
		assert(extension.acknowledge(int(state.renderer_generation), rows, mesh.surface_get_arrays(0)[Mesh.ARRAY_VERTEX]))
		displayed_tick = int(state.published_tick)
		captions[1].text = "PEER PID %d / GODOT PID %d / TICK %03d" % [state.source_pid, OS.get_process_id(), displayed_tick]
		captions[2].text = "%s POSE %02d / INPUT %d %s / CONFIRMED %.0f" % [["IDLE", "JUMP", "FAIR"][int(rows[3])], int(rows[4])+1, int(rows[13]), "PREDICTED" if rows[12] != 0 else "KNOWN", rows[21]]
		captions[3].text = "BAG %s / DAMAGE %.0f / STUN %.0f" % [["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][int(rows[37])], rows[8], rows[36]]
		captions[4].text = "SOURCE GEN %d / LOCAL SQL GEN %d / SKIPPED %d" % [state.source_generation, state.renderer_generation, state.skipped_generations]
		captions[5].text = "RESTORE %.0f / REPLAY %.0f / SOURCE WALL %.3fS" % [rows[18], rows[19]-1, state.source_elapsed_us / 1000000.0]
		captions[7].text = "IPC ROWS -> SQLITE -> ARRAYMESH: EXACT / ROWS %d" % (rows.size()/27)
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
	var current: Dictionary = JSON.parse_string(observation.current)
	var paused: bool = not faults and current.simulation_tick >= 92 and current.simulation_tick < 106
	if observation.has("frame"):
		var frame: Dictionary = observation.frame
		var state: Dictionary = JSON.parse_string(frame.status)
		var rows: PackedFloat64Array = frame.rows
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = frame.vertices
		arrays[Mesh.ARRAY_COLOR] = frame.colors
		mesh.clear_surfaces()
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, arrays)
		var uploaded := mesh.surface_get_arrays(0)
		assert(extension.acknowledge(int(state.renderer_generation), rows, uploaded[Mesh.ARRAY_VERTEX]))
		displayed_tick = int(state.published_tick)
		displayed_generation = int(state.renderer_generation)
		captions[2].text = "DISPLAY: %s POSE %02d / INPUT %s" % [["IDLE", "JUMP", "FAIR"][int(rows[3])], int(rows[4]) + 1, "PREDICTED" if rows[12] != 0 else "CONFIRMED"]
		captions[3].text = "DISPLAY BAG: %s / %.0f%% / STUN %.0f" % [["HOVERING", "HIT", "HITSTUN", "FALLING", "LANDED"][int(rows[37])], rows[8], rows[36]]
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
			assert(extension.finish_scheduled())
			print("SCHEDULE_CAPTURE_OK observed_video_frames=", video_frames)
			get_tree().quit(0)
			set_process(false)

func _inject_faults(current: Dictionary):
	if current.simulation_tick >= 60 and not injected[0]:
		injected[0] = true
		print("FAULT_BEGIN main")
		OS.delay_msec(800)
		print("FAULT_END main")
		var after: Dictionary = JSON.parse_string(extension.poll_scheduled(false).current)
		fault_note = "MAIN THREAD: 800 MS / WORKER ADVANCED %d TICKS" % (int(after.simulation_tick) - int(current.simulation_tick))
	elif current.simulation_tick >= 90 and not injected[1]:
		injected[1] = true
		print("PROCESS_STOP_READY")
		fault_note = "PROCESS STOP REQUESTED / EXTERNAL SIGSTOP + SIGCONT"
	elif current.simulation_tick >= 130 and not injected[2]:
		injected[2] = true
		RenderingServer.call_on_render_thread(_render_stall)
		RenderingServer.force_sync()
		var after: Dictionary = JSON.parse_string(extension.poll_scheduled(false).current)
		fault_note = "RENDER THREAD: 800 MS / WORKER ADVANCED %d TICKS" % (int(after.simulation_tick) - int(current.simulation_tick))

static func _render_stall():
	print("RENDER_THREAD_ID ", OS.get_thread_caller_id())
	print("FAULT_BEGIN render")
	OS.delay_msec(800)
	print("FAULT_END render")
