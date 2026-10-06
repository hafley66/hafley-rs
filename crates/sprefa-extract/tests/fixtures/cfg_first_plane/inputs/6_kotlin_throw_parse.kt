fun parseJson(input: String): String {
    try {
        mayThrow()
        throw RuntimeException("invalid JSON")
    } catch (e: Exception) {
        recoverHere()
    }
    return input
}
