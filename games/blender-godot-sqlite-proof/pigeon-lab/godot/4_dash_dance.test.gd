extends SceneTree

# Real PigeonSql dash dance through the runtime observer. The sibling
# 2_phase_view.test.gd covers the observer's own edge cases with authored
# samples; this drives actual start_controlled/advance_controlled/acknowledge
# and feeds the published phase columns into the same stage observer.

const Stage = preload("res://2_stage.gd")
const Rows = preload("res://1_rows_auto.gd")
const Payload = preload("res://1_payload_auto.gd")

# Dash right, reverse inside the initial dash, hold to run, turn, then jump on
# the tick after the turn begins, while TURN is still the current phase.
const TAPE := [
	[0, 6, 1.0, 0], [6, 26, -1.0, 0], [26, 27, 1.0, 0], [27, 28, 1.0, 1], [28, 40, 0.0, 0],
]

func stick(axis: float, buttons: int) -> Payload.ControlInput:
	var value := Payload.ControlInput.new()
	value.axis = axis
	value.buttons = buttons
	return value

func tape() -> Array:
	var inputs := []
	for segment in TAPE:
		for _t in range(segment[0], segment[1]):
			inputs.append(stick(segment[2], segment[3]))
	return inputs

func _initialize():
	call_deferred("dash_dance")

func dash_dance():
	var stage := Stage.new()
	root.add_child(stage)
	stage.set_process(false)
	stage.set_physics_process(false)
	assert(stage.controlled and not stage.control_demo)
	var inputs := tape()
	var observed := []
	for input in inputs:
		stage._control_step(input)
		observed.append([stage.motion_phase, stage.motion_enter_tick, stage.previous_motion_phase])
	# The first published sample is already DASH: IDLE is never presented here.
	assert(observed[0] == [2, 0, -1])
	assert(observed[5] == [2, 0, -1])
	# Reverse inside the initial dash: same phase, restarted clock, observed edge.
	assert(observed[6] == [2, 6, 2])
	assert(stage.observed_edges["DASH->DASH"] == 1)
	# Held through the rest of the initial dash without a second entry.
	for t in range(6, 21):
		assert(observed[t] == [2, 6, 2])
	assert(observed[21] == [3, 21, 2])
	assert(observed[26] == [5, 26, 3])
	# Jump pressed while TURN is current: the jump edge wins over the reverse
	# stick that would otherwise start another dash.
	assert(observed[27] == [6, 27, 5])
	assert(observed[30] == [11, 30, 6])
	assert(stage.observed_edges == {
		"DASH->DASH": 1, "DASH->RUN": 1, "RUN->TURN": 1,
		"TURN->SQUAT": 1, "SQUAT->JUMP": 1,
	})
	assert(stage.phase_nodes.keys() == [2, 3, 5, 6, 11])
	assert(stage.phase_graph.get_connection_list().size() == 5)
	assert(stage.phase_nodes[2].get_child(0).text == "entered T6 / re-entry")
	assert(stage.captions[8].text == "PHASE JUMP / PREV SQUAT / T30 / OBSERVED GRAPH")
	assert(stage.captions[9].text.begins_with("OBSERVED EDGES: DASH->DASH"))

	# Same tape read straight off the extension, keeping the published phase
	# clock next to the observer's derived entry tick.
	var replay := Stage.new()
	root.add_child(replay)
	replay.set_process(false)
	replay.set_physics_process(false)
	var samples := []
	for input in inputs:
		var frame := Payload.FramePayload.from_wire(replay.extension.advance_controlled(input.to_wire()))
		var meta := Rows.frame_values(frame.rows)
		replay._upload_and_acknowledge(frame, int(frame.status.renderer_generation))
		samples.append([int(frame.status.simulation_tick), meta.phase, meta.phase_ticks])
	assert(samples[0] == [0, 2.0, 1.0])
	assert(samples[5] == [5, 2.0, 6.0])
	# The restarted dash clock is what exposes the self-transition.
	assert(samples[6] == [6, 2.0, 1.0])
	assert(samples[20] == [20, 2.0, 15.0])
	assert(samples[21] == [21, 3.0, 1.0])
	assert(samples[26] == [26, 5.0, 1.0])
	assert(samples[27] == [27, 6.0, 1.0])
	# Three jumpsquat ticks, then takeoff.
	assert(samples[30] == [30, 11.0, 1.0])
	for sample in samples:
		replay._observe_phase(sample[0], sample[1], sample[2])
	assert(replay.motion_enter_tick == stage.motion_enter_tick)
	assert(replay.observed_edges == stage.observed_edges)
	# Re-presenting an earlier real sample rewinds the observed graph.
	replay._observe_phase(samples[6][0], samples[6][1], samples[6][2])
	assert(replay.observed_edges.is_empty())
	assert(replay.phase_graph.get_connection_list().is_empty())
	assert(replay.phase_nodes.keys() == [2])
	assert(replay.motion_phase == 2 and replay.previous_motion_phase == -1 and replay.motion_enter_tick == 6)

	stage.queue_free()
	replay.queue_free()
	print("DASH_DANCE_OK dash_reentry=T6 run=T21 turn=T26 squat=T27 takeoff=T30 edges=", stage.observed_edges.size())
	quit(0)
