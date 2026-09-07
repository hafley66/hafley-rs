extends SceneTree

func _initialize():
	call_deferred("verify")

func verify():
	var scene = load("res://1_model.glb").instantiate()
	root.add_child(scene)
	var skeletons = scene.find_children("*", "Skeleton3D", true, false)
	var meshes = scene.find_children("*", "MeshInstance3D", true, false)
	var players = scene.find_children("*", "AnimationPlayer", true, false)
	assert(skeletons.size() == 1)
	assert(meshes.size() == 1)
	assert(players.size() == 1)
	var skeleton = skeletons[0]
	var bone = skeleton.find_bone("Arm")
	assert(bone >= 0)
	var player = players[0]
	var clips = player.get_animation_list()
	var clip = ""
	for candidate in clips:
		if candidate != "RESET":
			clip = candidate
	assert(clip != "")
	player.play(clip)
	player.seek(0.0, true)
	var first = skeleton.get_bone_pose_rotation(bone)
	player.seek(0.5, true)
	var middle = skeleton.get_bone_pose_rotation(bone)
	assert(first.angle_to(middle) > 1.0)
	player.seek(0.0, true)
	assert(first.is_equal_approx(skeleton.get_bone_pose_rotation(bone)))
	print("GODOT_PROOF_OK meshes=%s bones=%s clip=%s angle=%s restored=true" % [meshes.size(), skeleton.get_bone_count(), clip, first.angle_to(middle)])
	quit(0)
