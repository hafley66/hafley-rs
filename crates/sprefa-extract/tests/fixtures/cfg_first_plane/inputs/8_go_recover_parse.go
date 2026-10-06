package p
func parseJson(input string) {
	defer func() { if recover() != nil { recoverHere() } }()
	mayThrow()
	panic("invalid JSON")
}
