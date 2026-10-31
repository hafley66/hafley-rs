// The ts slow tier's driver: the TypeScript 7.1 API (`tsrs/sync`, the TS SDK served by the
// `tsrs --api` binary) answers the demand ryi's parse found.
// @comment-ok: the seam's whole wire format, stated where both ends read it.
// stdin: {"root","package","server","tsi":bool,"files":[{"supplied","absolute",
//   "sites":[[start,end,name,call,typeRef]],"pairs":[[source,target]]}]}; offsets are UTF-8 bytes.
// stdout: one JSON object {"version","calls":{supplied:[ref]},"types":{supplied:[ref]},
//   "tsi":[{"relation","args"}],"coverage":[[relation,complete,detail]],"filesAnswered"};
//   ref = [start,end,name,dstPath,dstName,dstOffset], dstPath "" = outside the corpus.
// The API speaks UTF-16 offsets; every offset crosses `units` on the way in and `bytes` out.

import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const request = JSON.parse(fs.readFileSync(0, "utf8"));
const { API } = await import(pathToFileURL(path.join(request.package, "dist/api/sync/api.js")).href);
const { SyntaxKind } = await import(pathToFileURL(path.join(request.package, "dist/ast/index.js")).href);
const { version } = JSON.parse(fs.readFileSync(path.join(request.package, "package.json"), "utf8"));

const SymbolFlags = { Class: 32, Interface: 64, Enum: 384, TypeParameter: 262144, TypeAlias: 524288, Alias: 2097152, Optional: 16777216 };
const TypeFlags = { Union: 134217728, Intersection: 268435456, Object: 1048576 };
const ObjectFlags = { Class: 1, Interface: 2, Reference: 4 };
const PRIMITIVES = [
    [1, "any"], [2, "unknown"], [4, "undefined"], [8, "null"], [16, "void"],
    [32 | 1024, "string"], [64 | 2048, "number"], [128 | 4096, "bigint"],
    [256 | 8192, "boolean"], [512 | 16384, "symbol"], [262144, "never"],
];
const DECLARED = SymbolFlags.Class | SymbolFlags.Interface | SymbolFlags.TypeParameter | SymbolFlags.TypeAlias;
const DECLARED_TYPE = DECLARED | SymbolFlags.Enum;

/// UTF-8 byte offset <-> UTF-16 unit offset for one file's text.
function mapper(text) {
    const ascii = !/[^\x00-\x7f]/.test(text);
    if (ascii) return { units: (byte) => byte, bytes: (unit) => unit };
    const byteOfUnit = new Uint32Array(text.length + 1);
    let byte = 0;
    for (let unit = 0; unit < text.length; unit += 1) {
        byteOfUnit[unit] = byte;
        const code = text.charCodeAt(unit);
        if (code < 0x80) byte += 1;
        else if (code < 0x800) byte += 2;
        else if (code >= 0xd800 && code <= 0xdbff) { byteOfUnit[unit + 1] = byte; byte += 4; unit += 1; }
        else byte += 3;
    }
    byteOfUnit[text.length] = byte;
    return {
        units: (target) => {
            let low = 0, high = text.length;
            while (low < high) { const mid = (low + high) >> 1; if (byteOfUnit[mid] < target) low = mid + 1; else high = mid; }
            return low;
        },
        bytes: (unit) => byteOfUnit[unit],
    };
}

const corpus = new Map(request.files.map((file) => [file.absolute.toLowerCase(), file.supplied]));
const texts = new Map();
function textOf(file) {
    if (!texts.has(file)) {
        const text = fs.readFileSync(file, "utf8");
        texts.set(file, { text, map: mapper(text) });
    }
    return texts.get(file);
}

const api = new API({ cwd: request.root, tsserverPath: request.server });
const needed = request.files.filter((file) => file.sites.length > 0);
const snapshot = api.createSnapshot({ openFiles: needed.map((file) => file.absolute) });
const answers = { version: `tsrs ${version}`, calls: {}, types: {}, tsi: [], coverage: [], filesAnswered: needed.length };

/// The written type a type-reference token sits in (`Mapper<T>` in `implements Mapper<T>`):
/// the API answers `any` at the token itself, the node names the instantiation.
function writtenType(project, source, start) {
    let node = source, found;
    for (;;) {
        let next;
        node.forEachChild((child) => { if (!next && child.getStart(source) <= start && start < child.end) next = child; });
        if (!next) break;
        if ((next.kind === SyntaxKind.TypeReference || next.kind === SyntaxKind.ExpressionWithTypeArguments)
            && next.getStart(source) === start) found = next;
        node = next;
    }
    if (!found) return undefined;
    return found.kind === SyntaxKind.TypeReference
        ? project.checker.getTypeFromTypeNode(found)
        : project.checker.getTypeAtLocation(found);
}

/// The single declaration a reference resolves to, as [path (real case), nameStart, nameEnd, name]
/// in UTF-8 bytes; several declarations stay for the syntax leg, as the LSP definition did.
function definition(project, symbol) {
    if (!symbol) return undefined;
    const target = symbol.flags & SymbolFlags.Alias ? project.checker.getAliasedSymbol(symbol) : symbol;
    const declarations = target?.declarations ?? [];
    if (declarations.length !== 1) return undefined;
    const handle = declarations[0];
    const source = project.program.getSourceFile(handle.path);
    const node = handle.resolve(project);
    if (!source || !node) return undefined;
    const named = node.name ?? node;
    const start = named.getStart(source);
    const { text, map } = textOf(source.fileName);
    return { file: source.fileName, start: map.bytes(start), end: map.bytes(named.end), name: text.slice(start, named.end), symbol: target };
}

const definitions = new Map();
const perFile = new Map();
for (const file of needed) {
    const project = snapshot.getDefaultProjectForFile(file.absolute);
    if (!project) continue;
    const { map } = textOf(file.absolute);
    const positions = file.sites.map(([start]) => map.units(start));
    const symbols = project.checker.getSymbolAtPosition(file.absolute, positions);
    perFile.set(file, { project, positions, symbols });
    file.sites.forEach(([start, end, name, call, typeRef], index) => {
        const found = definition(project, symbols[index]);
        if (!found) return;
        const inCorpus = corpus.get(found.file.toLowerCase());
        const reference = [start, end, name, inCorpus ?? "", inCorpus ? found.name : `${path.basename(found.file)}:${found.name}`, found.start];
        if (call) (answers.calls[file.supplied] ??= []).push(reference);
        if (typeRef) (answers.types[file.supplied] ??= []).push(reference);
        definitions.set(`${file.supplied}\0${start}\0${end}`, [inCorpus ?? found.file, found.start, found.end]);
    });
}

if (request.tsi) {
    const rows = new Rows();
    for (const file of needed) {
        const entry = perFile.get(file);
        if (!entry) continue;
        const { project, positions } = entry;
        const types = project.checker.getTypeAtPosition(file.absolute, positions);
        const handles = [];
        let source;
        file.sites.forEach(([start, end], index) => {
            let type = types[index];
            let symbol = entry.symbols[index];
            if (symbol && symbol.flags & SymbolFlags.Alias) symbol = project.checker.getAliasedSymbol(symbol);
            const key = `${file.supplied}\0${start}\0${end}`;
            const site = definitions.get(key);
            const declaration = site && site[0] === file.supplied && site[1] === start;
            // A declaration names the declared type of its symbol; a type-reference token
            // the API answers `any` for names the written type around it.
            if (symbol && symbol.flags & DECLARED_TYPE && declaration) {
                type = project.checker.getDeclaredTypeOfSymbol(symbol) ?? type;
            } else if (symbol && symbol.flags & DECLARED_TYPE && (!type || type.flags === 1)) {
                source ??= project.program.getSourceFile(file.absolute);
                type = writtenType(project, source, positions[index])
                    ?? project.checker.getDeclaredTypeOfSymbol(symbol) ?? type;
            }
            if (!type) { handles.push(undefined); return; }
            const id = rows.type(project, type, true, 0);
            handles.push([type, id]);
            rows.emit("tsi.has_type", { span: [file.supplied, start, end] }, { id });
            if (site && declaration) rows.emit("tsi.origin", { id }, { atom: "ts" }, { span: site });
            if (symbol) {
                const symbolId = rows.symbol(symbol);
                rows.emit("tsi.denotes", { id: symbolId }, { id });
                if (site) rows.emit("tsi.origin", { id: symbolId }, { atom: "ts" }, { span: site });
            }
        });
        for (const [source, target] of file.pairs) {
            const from = handles[source], to = handles[target];
            if (from && to && project.checker.isTypeAssignableTo(from[0], to[0])) {
                rows.emit("tsi.assignable", { id: from[1] }, { id: to[1] }, { atom: "structural" });
            }
        }
    }
    answers.tsi = rows.unique();
    const relations = new Set([...answers.tsi.map((fact) => fact.relation),
        "tsi.assignable", "tsi.conforms", "tsi.subtype", "tsi.equivalent", "ts.mapped", "ts.conditional", "ts.readonly"]);
    const DETAIL = {
        "tsi.subtype": "the TS 7.1 API exposes no subtype or strict_subtype query; no rows produced",
        "tsi.equivalent": "the TS 7.1 API exposes no identical or comparable query; no rows produced",
        "tsi.assignable": "isTypeAssignableTo for requested site pairs only",
        "tsi.has_type": "getTypeAtPosition for requested spans only; a type-reference token answers the written type node around it",
        "ts.mapped": "mapped key/constraint/template decomposition is not requested by this tier; no rows produced",
        "ts.conditional": "conditional type decomposition is not requested by this tier; no rows produced",
        "ts.readonly": "readonly modifiers are not requested by this tier; no rows produced",
        "tsi.conforms": "getBaseTypes for requested class and interface types only; implements clauses and structural conformance are not enumerated",
    };
    answers.coverage = [...relations].sort().map((relation) => [relation, false,
        DETAIL[relation] ?? "TS 7.1 API type structure for requested sites only; dependency owners remain leaves; expansion stops after 32 levels"]);
}

snapshot.dispose();
api.close();
process.stdout.write(JSON.stringify(answers));

function Rows() {
    const facts = [];
    let next = 0;
    const typeIds = new Map(), symbolIds = new Map(), expanded = new Set();
    const id = () => next++;
    this.emit = (relation, ...args) => facts.push({ fact: 0, relation, args });
    this.unique = () => {
        const seen = new Set();
        return facts.filter((fact) => { const key = JSON.stringify(fact); return !seen.has(key) && seen.add(key); });
    };
    this.symbol = (symbol) => {
        if (symbolIds.has(symbol.id)) return symbolIds.get(symbol.id);
        const symbolId = id();
        symbolIds.set(symbol.id, symbolId);
        this.emit("tsi.symbol", { id: symbolId });
        this.emit("tsi.name", { id: symbolId }, { text: symbol.name ?? "" });
        return symbolId;
    };
    const inCorpus = (symbol) => (symbol?.declarations ?? []).some((handle) => corpus.has(handle.path.toLowerCase()));
    this.type = (project, type, expand, depth) => {
        expand = expand && depth < 32;
        const key = `${project.id}\0${type.id}`;
        let typeId = typeIds.get(key);
        if (typeId !== undefined) {
            if (!expand || expanded.has(key)) return typeId;
        } else {
            typeId = id();
            typeIds.set(key, typeId);
            this.emit("tsi.type", { id: typeId });
            this.emit("tsi.name", { id: typeId }, { text: project.checker.typeToString(type) });
        }
        if (expand) expanded.add(key);
        const primitive = PRIMITIVES.find(([mask]) => type.flags & mask);
        if (primitive) {
            this.emit("tsi.primitive", { id: typeId }, { atom: primitive[1] });
            return typeId;
        }
        if (!expand) return typeId;
        if (type.flags & (TypeFlags.Union | TypeFlags.Intersection)) {
            this.emit(type.flags & TypeFlags.Union ? "tsi.sum" : "tsi.product", { id: typeId });
            (type.getTypes() ?? []).forEach((part, position) => {
                const target = this.type(project, part, true, depth + 1);
                this.emit("tsi.edge", { id: id() }, { id: typeId }, { text: String(position) }, { id: target }, { int: position });
            });
        }
        if (type.flags & TypeFlags.Object) {
            this.emit("tsi.product", { id: typeId });
            if (type.objectFlags & ObjectFlags.Interface) this.emit("ts.interface", { id: typeId });
            let application;
            if ((type.aliasTypeArguments ?? []).length > 0) {
                const alias = type.getAliasSymbol();
                const generic = alias && project.checker.getDeclaredTypeOfSymbol(alias);
                if (generic) application = [generic, type.getAliasTypeArguments()];
            } else if (type.objectFlags & ObjectFlags.Reference && type.target && type.target !== type.id) {
                application = [type.getTarget(), project.checker.getTypeArguments(type)];
            }
            if (application) {
                const callee = this.type(project, application[0], false, depth + 1);
                const list = id();
                this.emit("tsi.called", { id: typeId }, { id: callee }, { id: list });
                application[1].forEach((argument, position) => {
                    this.emit("tsi.argument", { id: list }, { int: position }, { id: this.type(project, argument, true, depth + 1) });
                });
            }
            project.checker.getPropertiesOfType(type).forEach((property, position) => {
                const propertyType = project.checker.getTypeOfSymbol(property);
                if (!propertyType) return;
                // Expand structural callables; nominal library/dependency owners remain leaves.
                const owner = propertyType.getSymbol();
                const target = this.type(project, propertyType, !owner || inCorpus(owner), depth + 1);
                const edge = id();
                this.emit("tsi.edge", { id: edge }, { id: typeId }, { text: property.name ?? "" }, { id: target }, { int: position });
                if (property.flags & SymbolFlags.Optional) this.emit("ts.optional", { id: edge });
            });
            if (type.objectFlags & (ObjectFlags.Class | ObjectFlags.Interface)) {
                for (const base of project.checker.getBaseTypes(type) ?? []) {
                    this.emit("tsi.conforms", { id: typeId }, { id: this.type(project, base, false, depth + 1) }, { atom: "declared" });
                }
                (type.getTypeParameters() ?? []).forEach((parameter, position) => {
                    this.emit("tsi.parameter", { id: this.type(project, parameter, false, depth + 1) }, { id: typeId }, { int: position }, { atom: "unspecified" });
                });
            }
            const signatures = project.checker.getSignaturesOfType(type, 0) ?? [];
            if (signatures.length > 0) this.emit("tsi.callable", { id: typeId });
            signatures.forEach((signature, index) => {
                (signature.getTypeParameters() ?? []).forEach((parameter, position) => {
                    this.emit("tsi.parameter", { id: this.type(project, parameter, false, depth + 1) }, { id: typeId }, { int: index * 1000 + position }, { atom: "unspecified" });
                });
                const parameters = signature.getParameters() ?? [];
                if (parameters.length >= 1000) throw new Error("signature exceeds TSI position stride 1000");
                parameters.forEach((parameter, position) => {
                    const parameterType = project.checker.getTypeOfSymbol(parameter);
                    if (!parameterType) return;
                    this.emit("tsi.input", { id: typeId }, { int: index * 1000 + position }, { id: this.type(project, parameterType, true, depth + 1) });
                });
                const returned = project.checker.getReturnTypeOfSignature(signature);
                if (returned) this.emit("tsi.output", { id: typeId }, { int: index }, { id: this.type(project, returned, false, depth + 1) });
            });
        }
        return typeId;
    };
}
