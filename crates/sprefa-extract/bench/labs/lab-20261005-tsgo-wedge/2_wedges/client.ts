// node client.ts <lsp-open|lsp-noopen|api|api-batch|lsp-batch> <callers|relations> <corpus-root> <tsgo>
// Wire positions are UTF-16 offsets (JS string indexes); stdout is one JSON line.

import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";

const [wedge, question, corpusRoot, tsgo] = process.argv.slice(2);
const root = fs.realpathSync(corpusRoot);

type Pending = { resolve: (value: any) => void; reject: (error: Error) => void };

// Content-Length framed JSON-RPC 2.0, the framing both `--lsp` and `--api --async` speak.
function connect(args: string[]) {
    const child = spawn(tsgo, args, { cwd: root, stdio: ["pipe", "pipe", "inherit"] });
    let buffer = Buffer.alloc(0);
    let nextId = 1;
    const pending = new Map<number, Pending>();
    const stats = { requests: 0 };
    const write = (message: object) => {
        const body = Buffer.from(JSON.stringify({ jsonrpc: "2.0", ...message }), "utf8");
        child.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
        child.stdin.write(body);
    };
    child.stdout.on("data", (chunk: Buffer) => {
        buffer = Buffer.concat([buffer, chunk]);
        for (;;) {
            const headerEnd = buffer.indexOf("\r\n\r\n");
            if (headerEnd < 0) return;
            const header = buffer.subarray(0, headerEnd).toString("ascii");
            const length = Number(/Content-Length: (\d+)/i.exec(header)![1]);
            if (buffer.length < headerEnd + 4 + length) return;
            const message = JSON.parse(buffer.subarray(headerEnd + 4, headerEnd + 4 + length).toString("utf8"));
            buffer = buffer.subarray(headerEnd + 4 + length);
            if (message.method !== undefined && message.id !== undefined) {
                // Server -> client request (LSP configuration, capability registration, progress).
                const result = message.method === "workspace/configuration"
                    ? message.params.items.map(() => null)
                    : null;
                write({ id: message.id, result });
            } else if (message.id !== undefined) {
                if (process.env.TRACE) process.stderr.write(`<- ${JSON.stringify(message).slice(0, 300)}\n`);
                const waiter = pending.get(message.id)!;
                pending.delete(message.id);
                if (message.error) waiter.reject(new Error(JSON.stringify(message.error)));
                else waiter.resolve(message.result);
            }
        }
    });
    const exited = new Promise((resolve) => child.on("exit", resolve));
    return {
        stats,
        request(method: string, params: unknown): Promise<any> {
            stats.requests += 1;
            const id = nextId++;
            if (process.env.TRACE) process.stderr.write(`-> ${method} ${JSON.stringify(params).slice(0, 200)}\n`);
            write({ id, method, params });
            return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
        },
        notify(method: string, params: unknown) {
            write({ method, params });
        },
        async close() {
            child.stdin.end();
            child.kill("SIGKILL");
            await exited;
        },
    };
}

type Site = { file: string; offset: number; line: number };
type Target = { file: string; lines: Set<number> };

function lineOf(text: string, offset: number): number {
    let line = 1;
    for (let ix = 0; ix < offset; ix += 1) if (text.charCodeAt(ix) === 10) line += 1;
    return line;
}

const texts = new Map<string, string>();
function textOf(file: string): string {
    if (!texts.has(file)) texts.set(file, fs.readFileSync(file, "utf8"));
    return texts.get(file)!;
}

function lspPosition(text: string, offset: number) {
    const before = text.slice(0, offset);
    return { line: before.split("\n").length - 1, character: offset - (before.lastIndexOf("\n") + 1) };
}

// A definition answer (Location | Location[] | LocationLink[] | null) lands on the target declaration line.
function definesTarget(result: any, target: Target): boolean {
    const locations = result === null || result === undefined ? [] : Array.isArray(result) ? result : [result];
    return locations.some((location: any) => {
        const range = location.range ?? location.targetSelectionRange;
        return fs.realpathSync(fileURLToPath(location.uri ?? location.targetUri)) === target.file && target.lines.has(range.start.line + 1);
    });
}

// The text prefilter ryi's fast tier applies: a site written NAME( or NAME<..>(. The checker decides each edge.
// `callers` asks Route (5_Route.ts); `callers=NAME@PATH` asks any function NAME declared in PATH.
function callersQuestion() {
    const [name, declRelative] = question === "callers" ? ["Route", "packages/signals/src/5_Route.ts"] : question.slice("callers=".length).split("@");
    const listed = execFileSync("rg", ["-l", "--type", "ts", `\\b${name}\\s*(<[^>(]*>)?\\(`, "packages"], { cwd: root })
        .toString().trim().split("\n").sort();
    const sites: Site[] = [];
    for (const relative of listed) {
        const file = path.join(root, relative);
        const text = textOf(file);
        for (const match of text.matchAll(new RegExp(`(?<!function\\s+)\\b${name}\\s*(<[^>(]*>)?\\(`, "g"))) {
            sites.push({ file, offset: match.index, line: lineOf(text, match.index) });
        }
    }
    const declFile = path.join(root, declRelative);
    const lines = new Set<number>();
    for (const match of textOf(declFile).matchAll(new RegExp(`function ${name}\\b`, "g"))) lines.add(lineOf(textOf(declFile), match.index));
    return { files: listed.map((relative) => path.join(root, relative)), sites, target: { file: declFile, lines } };
}

const RELATION_TYPES: [string, string][] = [
    ["packages/signals/src/0_types.ts", "DepthLimit"],
    ["packages/signals/src/0_ProxyTree.ts", "ProxyTreeDepth"],
    ["packages/signals/src/4_Query.ts", "AsyncStatus"],
    ["packages/signals/src/3_Endpoint.ts", "Serializable"],
    ["packages/signals/src/3_Endpoint.ts", "EndpointRequest"],
];

function relationsQuestion() {
    const declarations = RELATION_TYPES.map(([relative, name]) => {
        const file = path.join(root, relative);
        const match = new RegExp(`export (type|interface) ${name}\\b`).exec(textOf(file))!;
        return { file, name, offset: match.index + match[0].length - name.length };
    });
    const pairs: [number, number][] = [];
    for (let source = 0; source < declarations.length; source += 1) {
        for (let target = 0; target < declarations.length; target += 1) {
            if (source !== target) pairs.push([source, target]);
        }
    }
    return { declarations, pairs };
}

function groupByFile<T extends { file: string }>(items: T[]): Map<string, { item: T; index: number }[]> {
    const groups = new Map<string, { item: T; index: number }[]>();
    items.forEach((item, index) => {
        if (!groups.has(item.file)) groups.set(item.file, []);
        groups.get(item.file)!.push({ item, index });
    });
    return groups;
}

// Source file binary format (internal/api/encoder): 44-byte header, 28-byte node records
// [kind, pos, end, next, parent, data, flags]; a node handle is "index.kind.path".
const KIND_IDENTIFIER = 79;
function declarationNameOffset(data: Buffer, index: number): number {
    const nodes = data.readUInt32LE(40);
    const record = (ix: number, field: number) => data.readUInt32LE(nodes + ix * 28 + field * 4);
    const count = (data.length - nodes) / 28;
    for (let child = index + 1; child < count; child += 1) {
        if (record(child, 4) < index) break;
        if (record(child, 4) === index && record(child, 0) === KIND_IDENTIFIER) return record(child, 1);
    }
    return record(index, 1);
}

// tspath.Path is lowercased on a case-insensitive file system; map it back to the real spelling.
function realOf(tsPath: string, known: string[]): string {
    return known.find((file) => file.toLowerCase() === tsPath.toLowerCase()) ?? tsPath;
}

async function main() {
    let requests = 0;
    let filesOpened = 0;
    let answers: unknown;
    const lspInit = async (rpc: ReturnType<typeof connect>) => {
        await rpc.request("initialize", {
            processId: process.pid,
            rootUri: pathToFileURL(root).href,
            workspaceFolders: [{ uri: pathToFileURL(root).href, name: path.basename(root) }],
            capabilities: {},
            initializationOptions: {},
        });
        rpc.notify("initialized", {});
    };
    if (wedge === "lsp-open" || wedge === "lsp-noopen") {
        const rpc = connect(["--lsp", "--stdio"]);
        await lspInit(rpc);
        if (question.startsWith("callers")) {
            const { files, sites, target } = callersQuestion();
            if (wedge === "lsp-open") {
                for (const file of files) {
                    rpc.notify("textDocument/didOpen", { textDocument: { uri: pathToFileURL(file).href, languageId: "typescript", version: 1, text: textOf(file) } });
                    filesOpened += 1;
                }
            }
            const edges: string[] = [];
            for (const site of sites) {
                const result = await rpc.request("textDocument/definition", { textDocument: { uri: pathToFileURL(site.file).href }, position: lspPosition(textOf(site.file), site.offset) });
                if (definesTarget(result, target)) edges.push(`${path.relative(root, site.file)}:${site.line}`);
            }
            answers = { sites: sites.length, edges: edges.sort() };
        } else {
            answers = "unreachable: LSP has no type-relation request";
        }
        requests = rpc.stats.requests;
        await rpc.close();
    } else if (wedge === "api") {
        const rpc = connect(["--api", "--async", "--cwd", root]);
        await rpc.request("initialize", null);
        if (question.startsWith("callers")) {
            const { files, sites, target } = callersQuestion();
            const snapshot = (await rpc.request("updateSnapshot", { openFiles: files })).snapshot;
            filesOpened = files.length;
            const edges: string[] = [];
            const sourceFiles = new Map<string, Buffer>();
            for (const [file, group] of groupByFile(sites)) {
                const project = (await rpc.request("getDefaultProjectForFile", { snapshot, file })).id;
                const symbols = await rpc.request("getSymbolsAtPositions", { snapshot, project, file, positions: group.map(({ item }) => item.offset) });
                for (let ix = 0; ix < group.length; ix += 1) {
                    let symbol = symbols[ix];
                    if (!symbol) continue;
                    if (symbol.flags & 2097152) {
                        symbol = await rpc.request("getAliasedSymbol", { snapshot, project, symbol: symbol.id });
                        if (!symbol) continue;
                    }
                    for (const handle of symbol.declarations ?? []) {
                        const [index, , ...rest] = handle.split(".");
                        const declPath = realOf(rest.join("."), [target.file, ...files]);
                        if (!sourceFiles.has(declPath)) {
                            const response = await rpc.request("getSourceFile", { snapshot, project, file: declPath });
                            sourceFiles.set(declPath, Buffer.from(response.data, "base64"));
                        }
                        const nameOffset = declarationNameOffset(sourceFiles.get(declPath)!, Number(index));
                        if (declPath === target.file && target.lines.has(lineOf(textOf(declPath), nameOffset))) {
                            edges.push(`${path.relative(root, group[ix].item.file)}:${group[ix].item.line}`);
                            break;
                        }
                    }
                }
            }
            answers = { sites: sites.length, edges: edges.sort() };
        } else {
            const { declarations, pairs } = relationsQuestion();
            const snapshot = (await rpc.request("updateSnapshot", { openFiles: [...new Set(declarations.map((d) => d.file))] })).snapshot;
            filesOpened = new Set(declarations.map((d) => d.file)).size;
            const project = (await rpc.request("getDefaultProjectForFile", { snapshot, file: declarations[0].file })).id;
            const types: number[] = new Array(declarations.length);
            for (const [file, group] of groupByFile(declarations)) {
                const answered = await rpc.request("getTypesAtPositions", { snapshot, project, file, positions: group.map(({ item }) => item.offset) });
                group.forEach(({ index }, ix) => { types[index] = answered[ix]?.id; });
            }
            const results: string[] = [];
            for (const [source, target] of pairs) {
                const assignable = await rpc.request("isTypeAssignableTo", { snapshot, project, source: types[source], target: types[target] });
                results.push(`${declarations[source].name}->${declarations[target].name}:${assignable}`);
            }
            answers = { relations: results };
        }
        requests = rpc.stats.requests;
        await rpc.close();
    } else if (wedge === "api-batch" || wedge === "lsp-batch") {
        const lsp = wedge === "lsp-batch";
        const rpc = lsp ? connect(["--lsp", "--stdio"]) : connect(["--api", "--async", "--cwd", root]);
        if (lsp) await lspInit(rpc);
        else await rpc.request("initialize", null);
        const batch = async (groups: Map<string, { item: { offset: number }; index: number }[]>, pairs: { source: number; target: number; relation: string }[], definition: boolean) => {
            const files = [...groups.keys()];
            let snapshot = 0;
            if (!lsp) {
                snapshot = (await rpc.request("updateSnapshot", { openFiles: files })).snapshot;
                filesOpened = files.length;
            }
            const params = {
                snapshot,
                definition,
                files: files.map((file) => ({ uri: pathToFileURL(file).href, positions: groups.get(file)!.map(({ item }) => lspPosition(textOf(file), item.offset)) })),
                pairs,
            };
            return rpc.request(lsp ? "ryi/batch" : "ryiBatch", params);
        };
        if (question.startsWith("callers")) {
            const { sites, target } = callersQuestion();
            const groups = groupByFile(sites);
            const result = await batch(groups, [], true);
            const edges: string[] = [];
            [...groups.keys()].forEach((file, fx) => {
                groups.get(file)!.forEach(({ item }, ix) => {
                    if (definesTarget(result.files[fx][ix].definition, target)) edges.push(`${path.relative(root, item.file)}:${item.line}`);
                });
            });
            answers = { sites: sites.length, edges: edges.sort() };
        } else {
            const { declarations, pairs } = relationsQuestion();
            const groups = groupByFile(declarations);
            // Flattened index over the request's positions, in file order.
            const flat = new Map<number, number>();
            let next = 0;
            for (const group of groups.values()) for (const { index } of group) flat.set(index, next++);
            const result = await batch(groups, pairs.map(([source, target]) => ({ source: flat.get(source)!, target: flat.get(target)!, relation: "assignable" })), false);
            answers = { relations: pairs.map(([source, target], ix) => `${declarations[source].name}->${declarations[target].name}:${result.pairs[ix]}`) };
        }
        requests = rpc.stats.requests;
        await rpc.close();
    } else {
        throw new Error(`unknown wedge ${wedge}`);
    }
    process.stdout.write(JSON.stringify({ wedge, question, requests, files_opened: filesOpened, answers }) + "\n");
}

await main();
