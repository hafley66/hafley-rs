// kotlin_module_resolve/model/Sibling.kt: the only file declaring `shared`,
// so the wildcard import binds it here through the module plane.

package com.acme.model

fun shared(): Int = 3
