import { Children, Indent, Refkey } from "@alloy-js/core";

// Printer combinators; gen/<lang>/0_nodes.tsx builds one Rule per grammar.json rule.
export type Rule =
  | { k: "lit"; s: string } | { k: "field"; n: string } | { k: "child" } | { k: "kw"; set: string[] }
  | { k: "nl" } | { k: "blank" } | { k: "seq"; xs: Rule[] } | { k: "choice"; xs: Rule[] } | { k: "rep"; x: Rule; min: number }
  | { k: "ref"; name: string; get: () => Rule };
export const lit = (s: string): Rule => ({ k: "lit", s });
export const field = (n: string): Rule => ({ k: "field", n });
export const child: Rule = { k: "child" };
export const kw = (set: string[]): Rule => ({ k: "kw", set });
export const nl: Rule = { k: "nl" };
export const blank: Rule = { k: "blank" };
export const seq = (...xs: Rule[]): Rule => ({ k: "seq", xs });
export const choice = (...xs: Rule[]): Rule => ({ k: "choice", xs });
export const never: Rule = choice();
export const opt = (x: Rule): Rule => choice(x, blank);
export const rep = (x: Rule, min: number): Rule => ({ k: "rep", x, min });
// hidden `_rule`, emitted once as H_<name> and referenced lazily (recursion allowed)
export const ref = (name: string, get: () => Rule): Rule => ({ k: "ref", name, get });

// src = the prop queue a value came from ("children", a field name, "keywords")
export type Tok = { lit: string } | { kw: string } | { val: Children; src: string } | { nl: true };
type Queues = Record<string, Children[]>;
type Pos = Record<string, number>;
const posKey = (p: Pos) => JSON.stringify(Object.entries(p).sort());

// Unparse: enumerate every way the rule consumes the prop queues (a parser over props).
// `active` holds ref@position pairs on the current path; re-entering one is left recursion.
function* run(r: Rule, q: Queues, p: Pos, active: Set<string>): Generator<[Pos, Tok[]]> {
  switch (r.k) {
    case "lit": yield [p, [{ lit: r.s }]]; return;
    case "nl": yield [p, [{ nl: true }]]; return;
    case "blank": yield [p, []]; return;
    case "field": case "child": {
      const n = r.k === "field" ? r.n : "children";
      const i = p[n] ?? 0;
      if (i < (q[n]?.length ?? 0)) yield [{ ...p, [n]: i + 1 }, [{ val: q[n][i], src: n }]];
      return;
    }
    case "kw": {
      const i = p.keywords ?? 0;
      const w = q.keywords?.[i];
      if (typeof w === "string" && r.set.includes(w)) yield [{ ...p, keywords: i + 1 }, [{ kw: w }]];
      return;
    }
    case "choice": for (const x of r.xs) yield* run(x, q, p, active); return;
    case "seq": yield* runSeq(r.xs, 0, q, p, active); return;
    case "rep": yield* runRep(r, 0, q, p, active); return;
    case "ref": {
      const key = `${r.name}@${posKey(p)}`;
      if (active.has(key)) return;
      yield* run(r.get(), q, p, new Set([...active, key]));
      return;
    }
  }
}
function* runSeq(xs: Rule[], i: number, q: Queues, p: Pos, active: Set<string>): Generator<[Pos, Tok[]]> {
  if (i === xs.length) { yield [p, []]; return; }
  for (const [p1, t1] of run(xs[i], q, p, active)) for (const [p2, t2] of runSeq(xs, i + 1, q, p1, active)) yield [p2, [...t1, ...t2]];
}
function* runRep(r: Extract<Rule, { k: "rep" }>, count: number, q: Queues, p: Pos, active: Set<string>): Generator<[Pos, Tok[]]> {
  for (const [p1, t1] of run(r.x, q, p, active)) {
    if (posKey(p1) === posKey(p)) continue; // no progress
    for (const [p2, t2] of runRep(r, count + 1, q, p1, active)) yield [p2, [...t1, ...t2]];
  }
  if (count >= r.min) yield [p, []];
}

export const isLit = (t: Tok | undefined, s: string) => !!t && (("lit" in t && t.lit === s) || ("kw" in t && t.kw === s));

// Per-language hand layer: whitespace, symbol declaration, scopes.
export interface Policy {
  space(kind: string, prev: Tok, t: Tok): boolean;
  lines: Set<string>; // one token per line, no braces
  block: Set<string>; // `{`, indented item per line, `}`
  list: Set<string>; // item per line, no braces
  accept?(kind: string, props: Record<string, any>, toks: Tok[]): boolean;
  declare?(kind: string, name: string, refkey: Refkey | undefined): Children;
  scope?(kind: string, body: Children): Children;
}

function inline(policy: Policy, kind: string, toks: Tok[]): Children[] {
  const out: Children[] = [];
  toks.forEach((t, i) => {
    if (i > 0 && policy.space(kind, toks[i - 1], t)) out.push(" ");
    out.push("lit" in t ? t.lit : "kw" in t ? t.kw : "val" in t ? t.val : <hbr />);
  });
  return out;
}

// groups start at each value token; literals and keywords attach to the group before them
function groups(toks: Tok[]): Tok[][] {
  const gs: Tok[][] = [];
  for (const t of toks) ("val" in t || gs.length === 0 ? gs.push([t]) : gs.at(-1)!.push(t));
  return gs;
}

function layout(policy: Policy, kind: string, toks: Tok[]): Children {
  if (policy.lines.has(kind)) return toks.map((t, i) => [i > 0 ? <hbr /> : "", inline(policy, kind, [t])]);
  if (policy.list.has(kind)) return groups(toks).map((g, i) => [i > 0 ? <hbr /> : "", inline(policy, kind, g)]);
  if (!policy.block.has(kind)) return inline(policy, kind, toks);
  const gs = groups(toks.slice(1, -1));
  if (gs.length === 0) return "{}";
  return ["{", <Indent hardline trailingBreak>{gs.map((g, i) => [i > 0 ? <hbr /> : "", inline(policy, kind, g)])}</Indent>, "}"];
}

const present = (v: unknown) => v !== undefined && v !== null && v !== false && v !== "";

export interface NodeProps {
  kind: string;
  props: Record<string, any>;
  rule: Rule;
  multi: string[];
  def?: string;
  scope?: boolean;
}

export function makeNode(policy: Policy) {
  return function Node(p: NodeProps) {
    const q: Queues = {};
    for (const [k, v] of Object.entries(p.props)) {
      if (!present(v) || k === "refkey") continue;
      q[k] = k === "children" || k === "keywords" || p.multi.includes(k) ? [v].flat(Infinity).filter(present) : [v];
    }
    if (p.def && policy.declare && typeof q[p.def]?.[0] === "string") {
      q[p.def][0] = policy.declare(p.kind, q[p.def][0] as string, p.props.refkey as Refkey | undefined);
    }
    let best: Tok[] | undefined;
    let bestSeparators = -1;
    let bestLits = Infinity;
    for (const [pos, toks] of run(p.rule, q, {}, new Set())) {
      if (Object.keys(q).some((k) => (pos[k] ?? 0) !== q[k].length)) continue;
      if (policy.accept && !policy.accept(p.kind, p.props, toks)) continue;
      const lits = toks.filter((t) => "lit" in t).length;
      // Preserve list separators and item terminators before minimizing other literals.
      const separators = toks.filter((t, i) => "lit" in t && (t.lit === ";" || (t.lit === "," && "val" in (toks[i - 1] ?? {}) && "val" in (toks[i + 1] ?? {})))).length;
      if (separators > bestSeparators || (separators === bestSeparators && lits < bestLits)) {
        [best, bestSeparators, bestLits] = [toks, separators, lits];
      }
    }
    if (!best) throw new Error(`${p.kind}: props ${Object.keys(q)} do not fit the grammar rule`);
    const body = layout(policy, p.kind, best);
    return p.scope && policy.scope ? policy.scope(p.kind, body) : body;
  };
}

export function Leaf(p: { kind: string; children: Children }) {
  return <>{p.children}</>;
}
