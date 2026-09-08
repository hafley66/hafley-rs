# Generated from 0_presentation.tsp; sha256:644d771e64f1650f62add21f95c9815731b03675ea7cfe5e517dce3ee2eef4a7
extends RefCounted

const STRIDE = 27

static func frame_values(rows: PackedFloat64Array, entity: int = 0) -> Dictionary:
	assert(rows.size() % STRIDE == 0, "Invalid packed row length")
	for start in range(0, rows.size(), STRIDE):
		if rows[start + 1] == 0 and rows[start + 2] == entity:
			return {
				"action": rows[start + 3],
				"pose": rows[start + 4],
				"root_x": rows[start + 5],
				"root_y": rows[start + 6],
				"root_z": rows[start + 7],
				"damage": rows[start + 8],
				"hits": rows[start + 9],
				"last_hit": rows[start + 10],
				"contact": rows[start + 11],
				"predicted": rows[start + 12],
				"input": rows[start + 13],
				"animation_x": rows[start + 14],
				"animation_y": rows[start + 15],
				"reserved_13": rows[start + 16],
				"reserved_14": rows[start + 17],
				"restored": rows[start + 18],
				"advances": rows[start + 19],
				"total_loads": rows[start + 20],
				"confirmed": rows[start + 21],
			}
	return {}

static func target_values(rows: PackedFloat64Array, entity: int = 0) -> Dictionary:
	assert(rows.size() % STRIDE == 0, "Invalid packed row length")
	for start in range(0, rows.size(), STRIDE):
		if rows[start + 1] == 1 and rows[start + 2] == entity:
			return {
				"x": rows[start + 3],
				"y": rows[start + 4],
				"z": rows[start + 5],
				"vx": rows[start + 6],
				"vy": rows[start + 7],
				"vz": rows[start + 8],
				"stun": rows[start + 9],
				"phase": rows[start + 10],
				"grounded": rows[start + 11],
			}
	return {}

static func hurt_values(rows: PackedFloat64Array, entity: int = 0) -> Dictionary:
	assert(rows.size() % STRIDE == 0, "Invalid packed row length")
	for start in range(0, rows.size(), STRIDE):
		if rows[start + 1] == 2 and rows[start + 2] == entity:
			return {
				"matrix": rows.slice(start + 3, start + 19),
				"offset_x": rows[start + 19],
				"offset_y": rows[start + 20],
				"offset_z": rows[start + 21],
				"stretch_x": rows[start + 22],
				"stretch_y": rows[start + 23],
				"stretch_z": rows[start + 24],
				"radius": rows[start + 25],
				"enabled": rows[start + 26],
			}
	return {}

static func attack_values(rows: PackedFloat64Array, entity: int = 0) -> Dictionary:
	assert(rows.size() % STRIDE == 0, "Invalid packed row length")
	for start in range(0, rows.size(), STRIDE):
		if rows[start + 1] == 3 and rows[start + 2] == entity:
			return {
				"x": rows[start + 3],
				"y": rows[start + 4],
				"z": rows[start + 5],
				"radius": rows[start + 6],
				"damage": rows[start + 7],
				"enabled": rows[start + 8],
			}
	return {}

