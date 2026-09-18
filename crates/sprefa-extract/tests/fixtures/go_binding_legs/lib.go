package bindlegs

// Each member name is shared with a decoy type so a bare name match is
// ambiguous: a receiver-typed leg is the only answer that binds.

type T struct{}

func (t *T) Method() {}

type MethodDecoy struct{}

func (d *MethodDecoy) Method() {}

type S struct{ f T }

// field leg: s.f reads field f of declared type T, then Method binds on T.
func fieldCase(s S) { s.f.Method() }

type Foo struct{}

func (f *Foo) Bar() {}

type BarDecoy struct{}

func (d *BarDecoy) Bar() {}

// ctor-return leg: x := NewFoo() binds x to *Foo, then Bar binds on Foo.
func NewFoo() *Foo { return &Foo{} }

func ctorCase() { x := NewFoo(); x.Bar() }

type Proj interface{ Project() }

// interface leg: the param p: Proj binds Project on the interface itself.
func ifaceCase(p Proj) { p.Project() }
