function Panel(props: {title: string}) {
    const shown = props.title;
    return <span text={shown} />;
}
function App(secret: string) {
    return <Panel title={secret} />;
}
