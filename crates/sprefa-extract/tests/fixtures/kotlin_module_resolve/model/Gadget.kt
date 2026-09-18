// kotlin_module_resolve/model/Gadget.kt: a second file in the same package.
// A bare `shared` declared here AND in Sibling.kt is ambiguous and binds
// nothing; a bare `lone` binds only here.

package com.acme.model

object Gadget {
    fun spin(): Int = 1
}

fun lone(): Int = 2
