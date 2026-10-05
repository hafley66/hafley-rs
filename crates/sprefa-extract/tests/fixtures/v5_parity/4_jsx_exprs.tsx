function Card({title, subtitle, note, label, opt, items}: any) {
    return <div>{title}{subtitle}{note}{label}{opt}{items}</div>;
}
function App(secret: string, fallback: string, backup: string, ok: boolean, guarded: boolean, bag: any, first: string) {
    return <Card title={ok ? secret : fallback} subtitle={secret ?? backup} note={(guarded && secret)} label={`hi ${secret}`} opt={bag?.secret} items={[first, secret]} />;
}
