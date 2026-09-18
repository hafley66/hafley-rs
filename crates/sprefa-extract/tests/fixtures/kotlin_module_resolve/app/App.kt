// kotlin_module_resolve/app/App.kt: declares `dupName` as well, making the
// same-package name ambiguous for Main.kt.

package com.acme.app

fun dupName(): Int = 9
