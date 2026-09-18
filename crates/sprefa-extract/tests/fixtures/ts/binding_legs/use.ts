import { makeFoo, ifaceCase } from "./lib.js";

class Impl {
    project(): void {}
}

function crossFileCtor(): void {
    const x = makeFoo();
    x.bar();
}

function crossFileGeneric(): void {
    ifaceCase<Impl>(new Impl());
}
