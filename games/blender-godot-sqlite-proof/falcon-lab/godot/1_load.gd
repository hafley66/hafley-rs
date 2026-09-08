extends SceneTree

func _initialize():
	if not ClassDB.class_exists("FalconSql"):
		assert(GDExtensionManager.load_extension("res://0_falcon.gdextension") == GDExtensionManager.LOAD_STATUS_OK)
	var extension = ClassDB.instantiate("FalconSql")
	assert(extension.proof_version() == "falcon-sql-gdext-1")
	print("GDEXT_LOAD_OK ", Engine.get_version_info().string)
	quit(0)
