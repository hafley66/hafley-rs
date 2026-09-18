// kotlin_receivers/lib.kt: the member table. Widget.run and Decoy.run make
// the bare name `run` corpus-ambiguous, so a receiver leg must name the file
// and the def span to bind the right one. Proj is the generic-bound leg's
// interface; Gadget the spelled-object leg's.

package acme

class Widget(val id: Int) {
    fun run(): Int = id
}

class Holder(val w: Widget)

object Gadget {
    fun spin(): Int = 1
}

interface Proj {
    fun project(): Int
}

class Decoy {
    fun run(): Int = 2
}

fun makeWidget(): Widget = Widget(1)
