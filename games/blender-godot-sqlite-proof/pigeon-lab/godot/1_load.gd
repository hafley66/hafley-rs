extends SceneTree

func _initialize():
	if not ClassDB.class_exists("PigeonSql"):
		assert(GDExtensionManager.load_extension("res://0_pigeon.gdextension") == GDExtensionManager.LOAD_STATUS_OK)
	var extension = ClassDB.instantiate("PigeonSql")
	assert(extension.proof_version() == "pigeon-sql-gdext-1")
	print("GDEXT_LOAD_OK ", Engine.get_version_info().string)
	quit(0)
