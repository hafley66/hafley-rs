extends Node3D

var extension
var mesh := ArrayMesh.new()
var captions: Array[Label] = []
var tick := -1
var remaining := 0
var video_frames := 0

func _ready():
	if not ClassDB.class_exists("FalconSql"):
		assert(GDExtensionManager.load_extension("res://0_falcon.gdextension") == GDExtensionManager.LOAD_STATUS_OK)
	extension = ClassDB.instantiate("FalconSql")
	assert(extension.proof_version() == "falcon-sql-gdext-1")
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
	captions[5].text = "3D LINE MESH / SQL ROWS + MESH UPLOAD VERIFIED"
	captions[9].text = "0.5X + HOLDS / SCRIPTED TRAVEL / PM + MELEE KB + RAPIER"
	print("GDEXT_STAGE_READY runtime=", Engine.get_version_info().string)

func _process(_delta):
	if remaining == 0:
		if tick == 179:
			assert(extension.finish())
			assert(video_frames == 824)
			print("GODOT_CAPTURE_OK frames=", video_frames)
			get_tree().quit(0)
			set_process(false)
			return
		tick += 1
		var frame: Dictionary = extension.next_frame()
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
		remaining = 60 if tick in [60, 78, 91, 97, 101, 117, 126, 179] else 2
	remaining -= 1
	video_frames += 1
