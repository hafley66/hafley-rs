// kotlin_untyped_receiver/lib.kt: the member table. Store.push is the one
// owner of the name push, so a typed receiver's navigation call binds at
// origin receiver.

package defs

class Store(val n: Int) {
    fun push(x: Int): Int = x
}
