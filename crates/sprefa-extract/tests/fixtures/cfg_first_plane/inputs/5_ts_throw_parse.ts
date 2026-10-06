function parseJson(stdout: string) {
  try {
    const text = stdout.trim();
    if (!text) throw new Error("empty output");
    return JSON.parse(text);
  } catch (e) {
    throw new Error("invalid JSON");
  }
}
