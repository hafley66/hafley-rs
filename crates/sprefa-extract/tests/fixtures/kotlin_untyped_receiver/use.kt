// kotlin_untyped_receiver/use.kt: the control d.push(1) carries a typed param
// receiver; w.push(2) and q.push(3) ride fn returns the receiver plane cannot
// type, so they must decline with no corpus answer at all; the free sole(4)
// keeps its name-match leg.

package use

import defs.Store

fun mk(): Store = Store(1)

fun other(): Int = 5

fun sole(x: Int): Int = x

fun f(d: Store) {
    d.push(1)
    val w = mk()
    w.push(2)
    val q = other()
    q.push(3)
    sole(4)
}
