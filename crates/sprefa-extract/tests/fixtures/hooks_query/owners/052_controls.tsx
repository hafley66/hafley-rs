async function outer() { function Inner() { useSync(); } return Inner; }
function Parent() { const Async = async () => useAsync(); return Async; }
const Render = React.memo(async () => useWrapped());
const object = { useMethod() { useObject(); } };
