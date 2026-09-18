// kotlin_module_resolve/app/Main.kt: every module-plane shape the resolve arms
// read. The import leg (named, aliased, wildcard), the same-package leg, and
// the type plane's field/impl candidates through the same legs.

package com.acme.app

import com.acme.model.Widget
import com.acme.model.makeWidget as build
import com.acme.model.*

fun main() {
    val widget: Widget = build(1)
    val w2 = makeWidget(2)
    val l = lone()
    val s = shared()
    val d = dupName()
    val h = appHelper()
    println(widget.id + w2.id + l + s + d + h + Gadget.spin())
}

class Panel(val view: Widget, val part: Gadget) : Widget(0)
