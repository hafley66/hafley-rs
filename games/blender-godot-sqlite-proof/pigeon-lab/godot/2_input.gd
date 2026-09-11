extends RefCounted

# Last-pressed-wins resolver for two opposing digital direction sources.
#
# A physical stick cannot occupy +1 and -1 at once. Digital keyboards can hold
# both, and summing them to 0 drops the reverse that dash dancing needs. When
# both sides are held, the side pressed most recently is authoritative; when a
# side is released the remaining side takes over. `reset` runs on focus loss and
# suspends output until a fresh press, so a key left physically held through a
# blur cannot leak a stale direction. OS key-repeat never calls `press`.

var last_dir := 0
var suspended := false

func press(dir: int) -> void:
	suspended = false
	last_dir = signi(dir)

func reset() -> void:
	last_dir = 0
	suspended = true

func axis(left: bool, right: bool) -> float:
	if suspended:
		return 0.0
	if left and right:
		return float(last_dir)
	if not left and not right:
		last_dir = 0
		return 0.0
	if left:
		last_dir = -1
		return -1.0
	last_dir = 1
	return 1.0
