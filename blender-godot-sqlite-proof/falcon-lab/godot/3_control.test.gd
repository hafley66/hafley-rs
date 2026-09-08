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
	assert("PLAYER Z -11.1" in stage.captions[2].text)
	key(KEY_D, false)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS +0.0" in stage.captions[1].text)
	assert("PLAYER Z -11.1" in stage.captions[2].text)
	key(KEY_A, true)
	stage._physics_process(1.0 / 60.0)
	assert("AXIS -1.0" in stage.captions[1].text)
	assert("PLAYER Z -12.0" in stage.captions[2].text)
	key(KEY_A, false)
	key(KEY_SPACE, true)
	stage._physics_process(1.0 / 60.0)
	assert("BUTTONS 1" in stage.captions[1].text)
	key(KEY_SPACE, false)
	stage._physics_process(1.0 / 60.0)
	key(KEY_J, true)
	stage._physics_process(1.0 / 60.0)
	assert("BUTTONS 2" in stage.captions[1].text)
	assert("FAIR POSE 01" in stage.captions[2].text)
	key(KEY_J, false)
	stage.queue_free()
	print("CONTROL_KEYBOARD_OK movement=right_stop_left jump=true attack=true mesh_ack=exact")
	quit(0)
