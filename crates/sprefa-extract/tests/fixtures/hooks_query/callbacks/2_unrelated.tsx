function makeListener() { each(() => useRead()); }
unknownFunction(() => useRead());
function notAComponent() { Promise.resolve().then(() => useState()); }
