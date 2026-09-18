function run(project: () => void): void {
    project();
}

function constCase(): void {
    const project = (): void => {};
    project();
}

function closureCase(items: Array<() => void>): void {
    items.forEach(project => project());
}
