package bindlegs

// Cross-file callers: the receiver legs resolve through the same package
// directory, not through any import.
func callField(s S) { s.f.Method() }

func callCtor() { x := NewFoo(); x.Bar() }

func callIface(p Proj) { p.Project() }
