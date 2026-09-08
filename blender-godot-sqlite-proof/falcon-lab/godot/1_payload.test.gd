extends SceneTree

const Payload = preload("res://1_payload_auto.gd")

func _init():
	var fixture := Payload.FixtureStatus.new()
	fixture.simulation_tick = 91
	fixture.published_generation = 92
	fixture.renderer_generation = 92
	fixture.held_generation = null
	fixture.held_damage = null
	fixture.fresh_tick91_damage = 18.0
	fixture.restored = [78]
	fixture.saved = [91]
	var scheduled := Payload.ScheduledFrameStatus.new()
	scheduled.renderer_generation = 93
	scheduled.held_generation = 92
	scheduled.held_damage = 0.0
	scheduled.fresh_tick91_damage = 18.0
	scheduled.restored = [78]
	var external := Payload.ExternalStatus.new()
	external.renderer_generation = 94
	external.ipc_sql_exact = true
	var controlled := Payload.ControlledStatus.new()
	controlled.input = Payload.ControlInput.new()
	controlled.input.axis = -1.0
	controlled.input.buttons = 3
	controlled.renderer_generation = 95
	for status in [fixture, scheduled, external, controlled]:
		var frame := Payload.FramePayload.new()
		frame.rows = PackedFloat64Array([91, 0, 7, 18.0])
		frame.vertices = PackedVector3Array([Vector3(1, 2, 3), Vector3(4, 5, 6)])
		frame.colors = PackedColorArray([Color.RED, Color.BLUE])
		frame.status = status
		var wire := frame.to_wire()
		var decoded := Payload.FramePayload.from_wire(wire)
		assert(decoded.to_wire() == wire)
		assert(decoded.status.get_script() == status.get_script())
		var receipt := Payload.MeshReceipt.new()
		receipt.generation = decoded.status.renderer_generation
		receipt.rows = decoded.rows
		receipt.vertices = decoded.vertices
		assert(Payload.MeshReceipt.from_wire(receipt.to_wire()).to_wire() == receipt.to_wire())
	print("TYPED_PAYLOAD_OK variants=4 packed_arrays=exact receipt=exact nullable=preserved")
	quit(0)
