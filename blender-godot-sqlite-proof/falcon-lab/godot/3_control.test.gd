extends SceneTree

const Stage = preload("res://2_stage.gd")

func key(code: Key, pressed: bool):
	var event := InputEventKey.new()
	event.physical_keycode = code
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _initialize():
	call_deferred("check_keyboard")

func check_keyboard():
	var stage := Stage.new()
	root.add_child(stage)
	stage.set_process(false)
	stage.set_physics_process(false)
	assert(stage.controlled and not stage.control_demo)
	key(KEY_D, true)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS +1.0" in stage.captions[1].text)
	assert("DASH POSE" in stage.captions[2].text)
	assert(stage.captions[6].visible and "SPEED +2.000" in stage.captions[6].text and "FACING RIGHT" in stage.captions[6].text)
	key(KEY_D, false)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS +0.0" in stage.captions[1].text)
	assert("BRAKE" in stage.captions[2].text)
	key(KEY_A, true)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS -1.0" in stage.captions[1].text)
	key(KEY_A, false)
	key(KEY_SPACE, true)
	stage._physics_process(1.0 / 60.0)
	assert("BUTTONS 1" in stage.captions[1].text)
	key(KEY_SPACE, false)
	assert("JUMP SQUAT" in stage.captions[2].text)
	for _i in range(5):
		stage._physics_process(1.0 / 60.0)
	key(KEY_J, true)
	stage._physics_process(1.0 / 60.0)
	assert("BUTTONS 2" in stage.captions[1].text)
	assert("FAIR POSE 01" in stage.captions[2].text)
	key(KEY_J, false)
	# Opposing digital directions: the most recently pressed side wins, so a
	# reverse reaches the reducer while the old key is still held.
	key(KEY_D, true)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS +1.0" in stage.captions[1].text)
	key(KEY_A, true)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS -1.0" in stage.captions[1].text)
	key(KEY_D, false)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS -1.0" in stage.captions[1].text)
	key(KEY_A, false)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS +0.0" in stage.captions[1].text)
	# Focus loss clears held and order state: a key left physically held does
	# not leak a direction, and a fresh press resumes it.
	key(KEY_D, true)
	key(KEY_A, true)
	stage._notification(NOTIFICATION_APPLICATION_FOCUS_OUT)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS +0.0" in stage.captions[1].text)
	key(KEY_D, false)
	key(KEY_A, false)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS +0.0" in stage.captions[1].text)
	key(KEY_D, true)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS +1.0" in stage.captions[1].text)
	key(KEY_D, false)
	stage.queue_free()
	print("CONTROL_KEYBOARD_OK movement=right_stop_left jump=true attack=true mesh_ack=exact")
	quit(0)
