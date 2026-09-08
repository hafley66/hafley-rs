extends SceneTree

const Rows = preload("res://1_rows_auto.gd")

func _init():
	var packed := PackedFloat64Array()
	# Deliberately reorder rows: lookup follows kind/entity, not array position.
	for kind in [3, 1, 0, 2]:
		packed.append_array(PackedFloat64Array([91, kind, 7]))
		for index in range(24):
			packed.append(index + 0.25)
	var frame := Rows.frame_values(packed, 7)
	assert(frame.size() == 19)
	assert(frame.damage == 5.25 and frame.confirmed == 18.25)
	assert(Rows.frame_values(packed).is_empty())
	assert(Rows.target_values(packed, 7) == {
		"x": 0.25, "y": 1.25, "z": 2.25, "vx": 3.25, "vy": 4.25,
		"vz": 5.25, "stun": 6.25, "phase": 7.25, "grounded": 8.25,
	})
	assert(Rows.attack_values(packed, 7) == {
		"x": 0.25, "y": 1.25, "z": 2.25, "radius": 3.25, "damage": 4.25, "enabled": 5.25,
	})
	var hurt := Rows.hurt_values(packed, 7)
	assert(hurt.matrix.size() == 16 and hurt.matrix[15] == 15.25)
	assert(hurt.offset_x == 16.25 and hurt.enabled == 23.25)
	print("NAMED_ROWS_OK kinds=4 reordered=true entity_filter=true")
	quit(0)
