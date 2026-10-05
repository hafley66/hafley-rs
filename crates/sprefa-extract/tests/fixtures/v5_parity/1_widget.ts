function format(value: string): string { return value; }
class Widget {
    render(name: string): string {
        const label = format(name);
        return label;
    }
}
