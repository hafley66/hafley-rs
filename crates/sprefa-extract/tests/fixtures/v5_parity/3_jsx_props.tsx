function Card({title}: {title: string}) {
    const shown = title;
    return <div label={shown} />;
}
function App(secret: string, other: string) {
    return <Card title={secret} note={other} />;
}
