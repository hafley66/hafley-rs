export class Gap extends Error {
  constructor(public reason: string, detail: string) { super(detail); }
}
