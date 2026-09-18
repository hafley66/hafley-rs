export class T {
    m(): void {}
}

export class S {
    f: T;
    run(): void {
        this.f.m();
    }
}

export class Foo {
    bar(): void {}
}

export function makeFoo(): Foo {
    return new Foo();
}

export function ctorCase(): void {
    const x = makeFoo();
    x.bar();
}

export interface Proj {
    project(): void;
}

export function ifaceCase<P extends Proj>(p: P): void {
    p.project();
}
