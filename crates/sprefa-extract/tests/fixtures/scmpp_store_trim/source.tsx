export function Panel({ items }: { items: string[] }) {
  const [n, setN] = useState(0);
  if (n > 1) {
    useEffect(() => setN(n + 1));
  }
  for (const item of items) {
    console.log(item);
  }
  return <div onClick={() => setN(n + 1)}>{items.map((x) => <span key={x}>{x}</span>)}</div>;
}
const other = (a: number, b: number, c: number) => a + b + c;
