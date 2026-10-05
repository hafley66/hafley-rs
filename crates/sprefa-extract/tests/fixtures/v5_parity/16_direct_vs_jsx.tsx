function Card({title}: any) { return title; }
function App(x: string) {
    const direct = Card({title: x});
    return <Card title={x} />;
}
