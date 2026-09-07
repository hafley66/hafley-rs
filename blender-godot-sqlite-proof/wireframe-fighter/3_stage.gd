extends Node3D

var player: AnimationPlayer
var skeleton: Skeleton3D
var frame := 0
var initial: Quaternion
var caption: Label

func _ready():
	if not ResourceLoader.exists("res://2_wire_fighter.glb"):
		push_error("Wire fighter glTF import missing")
		get_tree().quit(2)
		return
	var model = load("res://2_wire_fighter.glb").instantiate()
	add_child(model)
	var skeletons = model.find_children("*", "Skeleton3D", true, false)
	var players = model.find_children("*", "AnimationPlayer", true, false)
	assert(skeletons.size() == 1 and players.size() == 1)
	skeleton = skeletons[0]
	assert(skeleton.get_bone_count() == 16)
	player = players[0]
	assert(player.has_animation("PunchKnee"))
	player.play("PunchKnee")
	player.pause()
	player.seek(0, true)
	initial = skeleton.get_bone_pose_rotation(skeleton.find_bone("thigh_R"))
	player.seek(2.0, true)
	assert(initial.angle_to(skeleton.get_bone_pose_rotation(skeleton.find_bone("thigh_R"))) > 1.0)
	player.seek(0, true)
	assert(initial.is_equal_approx(skeleton.get_bone_pose_rotation(skeleton.find_bone("thigh_R"))))
	print("WIRE_FIGHTER_TEST_OK bones=16 animated_knee=true restored=true")
	var camera = Camera3D.new()
	add_child(camera)
	camera.position = Vector3(3, 2.1, 5)
	camera.look_at(Vector3(0, 1.05, 0))
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.size = 3.3
	camera.current = true
	var environment = WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.background_mode = Environment.BG_COLOR
	environment.environment.background_color = Color("080c18")
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color.WHITE
	environment.environment.ambient_light_energy = .5
	add_child(environment)
	var light = DirectionalLight3D.new()
	add_child(light)
	light.rotation_degrees = Vector3(-45, -30, 0)
	var floor_instance = MeshInstance3D.new()
	floor_instance.mesh = PlaneMesh.new()
	floor_instance.mesh.size = Vector2(5, 5)
	var material = StandardMaterial3D.new()
	material.albedo_color = Color("11192c")
	floor_instance.material_override = material
	floor_instance.position.y = -.035
	add_child(floor_instance)
	var canvas = CanvasLayer.new()
	add_child(canvas)
	caption = Label.new()
	canvas.add_child(caption)
	caption.position = Vector2(24, 90)
	caption.add_theme_font_size_override("font_size", 19)

func _process(_delta):
	if player == null:
		return
	var t = float(frame % 90) / 30.0
	player.seek(t, true)
	caption.text = "WIREFRAME FIGHTER | 16 BONES | %.2f s" % t
	frame += 1
