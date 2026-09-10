extends SceneTree

# Headless geometry and label proof for the playable debug overlay. Builds the
# overlay without the native extension or any rendering: captions, the observed
# graph and the web touch bar are laid out for a desktop and a portrait phone,
# then asserted against physical text/touch minimums and lifecycle spacing.

const Stage = preload("res://2_stage.gd")

func _initialize():
	call_deferred("check_overlay")

func build_stage():
	var stage = Stage.new()
	stage.canvas_layer = CanvasLayer.new()
	stage.add_child(stage.canvas_layer)
	stage._create_captions()
	stage.phase_graph = GraphEdit.new()
	stage.canvas_layer.add_child(stage.phase_graph)
	stage.touch_bar = HBoxContainer.new()
	stage.canvas_layer.add_child(stage.touch_bar)
	for _i in range(6):
		var button := Button.new()
		button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		stage.touch_bar.add_child(button)
	return stage

func check_overlay():
	var stage = build_stage()
	# Real observer samples create lifecycle nodes and non-color markers.
	stage._observe_phase(10, 0.0, 1.0)
	stage._observe_phase(11, 2.0, 1.0)
	assert(stage.phase_nodes[2].title == "▶ DASH ←")
	assert(stage.phase_nodes[0].title == "IDLE")
	assert(stage.phase_nodes[2].self_modulate != stage.phase_nodes[0].self_modulate)
	assert(stage.phase_label(2, 0, 11) == "PHASE DASH / PREV IDLE / T11 / OBSERVED GRAPH")
	assert(stage.edges_label(["IDLE->DASH"], 2) == "OBSERVED EDGES: IDLE->DASH / NEW DASH")
	assert(stage.velocity_label(2.3, -0.1, 4) == "VEL X +2.300 / VY -0.100 / PHASE AGE 4 TICKS")

	stage._build_debug_panel()
	assert(stage.debug_panel is ScrollContainer)
	assert(stage.debug_rows.get_child_count() == stage.captions.size() + 1)

	for window in [Vector2(960, 540), Vector2(390, 844)]:
		stage._layout_debug_overlay(window)
		var scale: float = stage.debug_scale
		assert(absf(scale - Stage.debug_scale_for(window)) < 0.0001)
		for caption in stage.captions:
			assert(caption.get_theme_font_size("font_size") * scale >= 14.0 - 0.001)
		for button in stage.touch_bar.get_children():
			assert(button.custom_minimum_size.y * scale >= 44.0 - 0.001)
		for id in stage.phase_nodes:
			var node: GraphNode = stage.phase_nodes[id]
			assert(node.get_theme_font_size("title_font_size") * scale * Stage.DEBUG_GRAPH_ZOOM >= 14.0 - 0.001)
		assert(stage.debug_panel.size.x <= 960.0 and stage.debug_panel.size.y <= 540.0)
		var seen := {}
		var geometry: Dictionary = stage.debug_geometry
		for id in geometry["cells"]:
			assert(not seen.has(geometry["cells"][id]))
			seen[geometry["cells"][id]] = id
		assert(seen.size() == 12)
		assert(stage.phase_graph_cell(0).x < stage.phase_graph_cell(6).x)
		assert(stage.phase_graph_cell(6).x < stage.phase_graph_cell(9).x)
		assert(stage.phase_graph_cell(9).x < stage.phase_graph_cell(8).x)
		assert(absf(stage.phase_graph.zoom - Stage.DEBUG_GRAPH_ZOOM) < 0.0001)

	stage.queue_free()
	print("OVERLAY_GEOMETRY_OK desktop=960x540 mobile=390x844 text_px=14 touch_px=44 cells=12")
	quit(0)
