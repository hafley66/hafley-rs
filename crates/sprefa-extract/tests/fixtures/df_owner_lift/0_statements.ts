function outer() {
  function inner() { readNamed(); }
  label: { readLabel(); }
  try { readTry(); } catch { readCatch(); } finally { readFinally(); }
}
