@tool
class_name Patrol
extends Node2D

signal reached(waypoint: String)

enum State { IDLE, WALKING, WAITING }

const SPEED := 120.0
const WAYPOINTS := ["start", "mid", "end"]

@export var speed: float = SPEED

var state: State = State.IDLE


func _ready() -> void:
	state = State.WALKING
	for waypoint in WAYPOINTS:
		if waypoint == "mid":
			reached.emit(waypoint)
		else:
			print("skipping %s" % waypoint)


func step(delta: float) -> Vector2:
	match state:
		State.IDLE:
			return Vector2.ZERO
		State.WALKING:
			return Vector2.RIGHT * speed * delta
		_:
			return Vector2.ZERO


func make_counter() -> Callable:
	var count := 0
	return func() -> int:
		count += 1
		return count