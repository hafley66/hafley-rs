// kotlin_module_resolve/app/Helper.kt: a second file of Main.kt's own
// package; the same-package leg binds a bare name declared only here. It also
// declares `dupName`, which App.kt declares too: ambiguous in the package and
// bound by nothing.

package com.acme.app

fun appHelper(): Int = 7

fun dupName(): Int = 8
