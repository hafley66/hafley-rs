fun walk(items: List<Int>): Int {
    var total = 0
    for (item in items) {
        if (item < 0) {
            continue
        }
        if (item > 100) {
            break
        }
        total += item
    }
    when (total) {
        0 -> return -1
        else -> throw RuntimeException("x")
    }
}
