extends SceneTree

var player: AnimationPlayer
var label: Label
var frame := 0

func _initialize():
	call_deferred("setup")

func setup():
	var stage = Node3D.new()
	root.add_child(stage)
	var model = load("res://1_model.glb").instantiate()
	stage.add_child(model)
	player = model.find_children("*", "AnimationPlayer", true, false)[0]
	player.play("Swing")
	player.pause()
	var camera = Camera3D.new()
	stage.add_child(camera)
	camera.position = Vector3(4, 3, 5)
	camera.look_at(Vector3(0, 0.8, 0))
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 4.5
	camera.current = true
	var light = DirectionalLight3D.new()
	stage.add_child(light)
	light.rotation_degrees = Vector3(-45, -30, 0)
	light.light_energy = 1.5
	var environment = WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color("182333")
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = 0.6
	stage.add_child(environment)
	var floor_mesh = MeshInstance3D.new()
	floor_mesh.mesh = PlaneMesh.new()
	floor_mesh.mesh.size = Vector2(5, 5)
	var material = StandardMaterial3D.new()
	material.albedo_color = Color("35465a")
	floor_mesh.material_override = material
	floor_mesh.position.y = -0.03
	stage.add_child(floor_mesh)
	label = Label.new()
	label.position = Vector2(24, 100)
	label.add_theme_font_size_override("font_size", 20)
	root.add_child(label)
	var driver = Node.new()
	driver.set_script(load("res://0_capture_driver.gd"))
	driver.consume = render_frame
	root.add_child(driver)

func render_frame():
	if player == null:
		return false
	var phase = frame % 90
	var clip_time = minf(float(phase) / 60.0, 1.0)
	player.seek(clip_time, true)
	label.text = "BLENDER > GODOT | seek %.2fs | %s" % [clip_time, "START POSE" if phase >= 60 else "BONE ANIMATION"]
	frame += 1
	return false
