// kotlin_module_resolve/model/Sibling.kt: declares `shared` too, making the
// same-package name ambiguous for Main.kt.

package com.acme.model

fun shared(): Int = 3
