function useHook(x: string) { return x; }
function Card({title}: any) { return title; }
function App(x: string) {
    return <Card key="k" ref={x} title={useHook(x)} {...{extra: x}}>kid</Card>;
}
