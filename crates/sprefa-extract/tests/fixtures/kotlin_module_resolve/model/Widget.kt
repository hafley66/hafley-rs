// kotlin_module_resolve/model/Widget.kt: a top-level fun, a class, and a val
// the other package's imports bind through the module plane.

package com.acme.model

class Widget(val id: WidgetId)

fun makeWidget(id: WidgetId): Widget = Widget(id)

val DEFAULT_WIDGET: Widget = Widget(0)
