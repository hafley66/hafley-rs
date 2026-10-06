async function outer() { take(async () => readClosure()); take(function () { readNested(); }); return <Child value={readAttribute()} />; }
