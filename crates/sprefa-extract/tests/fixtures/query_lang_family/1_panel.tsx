export function Panel() {
  const [n, setN] = useState(0);
  return <div onClick={() => setN(n + 1)}>{useLabel(n)}</div>;
}

export function After() {
  return useAfter();
}
