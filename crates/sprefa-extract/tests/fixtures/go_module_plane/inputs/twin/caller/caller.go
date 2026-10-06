package caller

import (
	"example.com/c/debug"
	p "example.com/c/poison"
	q "example.com/c/other"
)

func Use() int {
	return debug.Helper() + q.Helper() + p.Pick()
}
