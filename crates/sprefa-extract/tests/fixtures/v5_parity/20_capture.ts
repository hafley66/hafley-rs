function consume(fn: () => string) { return fn(); }
function outer(value: string) { return consume(() => value); }
