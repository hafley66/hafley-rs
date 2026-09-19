// kotlin_module_resolve/model/Gadget.kt: a second file in the same package.
// A bare `lone` is declared only here and binds here through the wildcard
// import; `Gadget.spin()` is the receiver-plane member call.

package com.acme.model

object Gadget {
    fun spin(): Int = 1
}

fun lone(): Int = 2
