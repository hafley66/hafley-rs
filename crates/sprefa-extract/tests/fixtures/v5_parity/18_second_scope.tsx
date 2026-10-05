function Card({title}: any) { return title; }
function Hidden() {
    const Card = ({title}: any) => title;
    return Card;
}
function App(x: string) { return <Card title={x} />; }
