extends Node

var consume: Callable

func _process(_delta):
	consume.call()
