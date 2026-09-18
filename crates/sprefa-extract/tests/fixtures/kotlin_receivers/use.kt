// kotlin_receivers/use.kt: one navigation-call site per receiver leg.
// paramLeg/ctorLeg/fieldLeg/boundLeg/objectLeg bind through the receiver
// plane; returnLeg (constructor-return through a fn's declared return type)
// and shadow (a param named like the corpus fn) must decline with reason
// inferred in K1. Inner.self() exercises implicit this.

package acme

fun paramLeg(w: Widget) = w.run()

fun ctorLeg() {
    val w = Widget(2)
    w.run()
}

fun returnLeg() {
    val w = makeWidget()
    w.run()
}

fun fieldLeg(h: Holder) = h.w.run()

fun <P : Proj> boundLeg(p: P) = p.project()

fun objectLeg() = Gadget.spin()

class Inner(val id: Int) {
    fun me() = this.id
    fun self() = run()
    fun run() = 0
}

fun shadow(run: () -> Int) = run()
