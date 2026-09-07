extends SceneTree

var frames: Array
var moving: MeshInstance3D
var moving_material: StandardMaterial3D
var caption: Label
var frame_index := 0

func _initialize():
	call_deferred("setup")

func setup():
	var args = OS.get_cmdline_user_args()
	if args.size() != 1:
		push_error("Expected absolute Rust trace JSON path")
		quit(2)
		return
	var data = JSON.parse_string(FileAccess.get_file_as_string(args[0]))
	if not data is Dictionary or not data.get("frames") is Array:
		push_error("Invalid Rust trace")
		quit(2)
		return
	frames = data.frames
	if frames.is_empty():
		quit(2)
		return
	print("COLLISION_TRACE frames=%s fps=%s sha256=%s" % [frames.size(), data.get("fps"), FileAccess.get_sha256(args[0])])
	root.size = Vector2i(800, 600)
	root.content_scale_size = Vector2i(800, 600)
	var stage = Node3D.new()
	root.add_child(stage)
	var camera = Camera3D.new()
	stage.add_child(camera)
	camera.position = Vector3(3, 2.5, 6)
	camera.look_at(Vector3(0.5, 0.8, 0))
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 4.8
	camera.current = true
	var light = DirectionalLight3D.new()
	stage.add_child(light)
	light.rotation_degrees = Vector3(-40, -30, 0)
	var environment = WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color("182333")
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 0.6
	stage.add_child(environment)
	moving = MeshInstance3D.new()
	moving.mesh = CapsuleMesh.new()
	moving_material = StandardMaterial3D.new()
	moving.material_override = moving_material
	stage.add_child(moving)
	var target = MeshInstance3D.new()
	target.mesh = CapsuleMesh.new()
	var target_material = StandardMaterial3D.new()
	target_material.albedo_color = Color("43a2ce")
	target.material_override = target_material
	stage.add_child(target)
	pose_capsule(target, Vector3(1, 0.3, 0), Vector3(1, 1.3, 0), 0.25)
	var canvas = CanvasLayer.new()
	root.add_child(canvas)
	caption = Label.new()
	canvas.add_child(caption)
	caption.position = Vector2(24, 100)
	caption.size = Vector2(750, 100)
	caption.add_theme_font_size_override("font_size", 20)
	var driver = Node.new()
	driver.set_script(load("res://0_capture_driver.gd"))
	driver.consume = render_frame
	root.add_child(driver)

func pose_capsule(node: MeshInstance3D, a: Vector3, b: Vector3, radius: float):
	node.mesh.radius = radius
	node.mesh.height = a.distance_to(b) + 2.0 * radius
	node.position = (a + b) * 0.5
	var direction = b - a
	if direction.length_squared() > 0.000001:
		node.quaternion = Quaternion(Vector3.UP, direction.normalized())

func render_frame():
	if moving == null:
		return false
	if frame_index >= frames.size():
		print("COLLISION_RENDER_OK frames=%s" % frame_index)
		quit(0)
		return false
	var sample = frames[frame_index]
	pose_capsule(moving, Vector3(sample.a[0], sample.a[1], sample.a[2]), Vector3(sample.b[0], sample.b[1], sample.b[2]), sample.radius)
	moving_material.albedo_color = Color("ff6544") if sample.hit else Color("f5c34c")
	caption.text = "RUST > GODOT | Tick %s | %s" % [sample.tick, "CONTACT" if sample.hit else "SEPARATED"]
	frame_index += 1
	return false
