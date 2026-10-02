import { Output, render } from "@alloy-js/core";
import { describe, expect, it, beforeAll, afterAll } from "vitest";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { ReplaceFile, AutoZone, ManualZone } from "./3_ReplaceFile.js";

const TMP_DIR = join(import.meta.dirname, "../../test-output-replace");

function findFile(node: any, path: string): any {
  for (const item of node.contents) {
    if (item.kind === "file" && item.path === path) return item;
    if (item.kind === "directory") {
      const found = findFile(item, path);
      if (found) return found;
    }
  }
  return null;
}

function emitWithExisting(existingContent: string | null, zones: { id: string; content: string }[], manual?: string) {
  let existingFile: string | undefined;
  if (existingContent !== null) {
    mkdirSync(TMP_DIR, { recursive: true });
    existingFile = join(TMP_DIR, "test.rs");
    writeFileSync(existingFile, existingContent);
  }

  const res = render(
    <Output>
      <CrateDirectory>
        <ReplaceFile path="test.rs" existingFile={existingFile}>
          {zones.map(z => (
            <AutoZone id={z.id}>{z.content}</AutoZone>
          ))}
          {manual !== undefined && <ManualZone>{manual}</ManualZone>}
        </ReplaceFile>
      </CrateDirectory>
    </Output>
  );
  return findFile(res, "test.rs").contents;
}

beforeAll(() => mkdirSync(TMP_DIR, { recursive: true }));
afterAll(() => rmSync(TMP_DIR, { recursive: true, force: true }));

describe("ReplaceFile spliceZones", () => {
  it("first emit: builds zones + manual content", () => {
    const result = emitWithExisting(null, [
      { id: "http", content: "fn handler_http() {}" },
    ], "fn handler() { todo!() }");

    expect(result).toMatchInlineSnapshot(`
      "// alloy-http-start
      fn handler_http() {}
      // alloy-http-end

      fn handler() { todo!() }
      "
    `);
  });

  it("re-emit same zones: replaces auto content, preserves manual", () => {
    const existing = [
      "// alloy-http-start",
      "fn old_handler_http() {}",
      "// alloy-http-end",
      "",
      "fn handler() { real_impl() }",
    ].join("\n");

    const result = emitWithExisting(existing, [
      { id: "http", content: "fn new_handler_http() {}" },
    ]);

    expect(result).toMatchInlineSnapshot(`
      "// alloy-http-start
      fn new_handler_http() {}
      // alloy-http-end

      fn handler() { real_impl() }
      "
    `);
  });

  it("re-emit with added zone: appends new zone before manual content", () => {
    const existing = [
      "// alloy-http-start",
      "fn handler_http() {}",
      "// alloy-http-end",
      "",
      "fn handler() { real_impl() }",
    ].join("\n");

    const result = emitWithExisting(existing, [
      { id: "http", content: "fn handler_http() {}" },
      { id: "ws", content: "fn handler_ws() {}" },
    ]);

    expect(result).toMatchInlineSnapshot(`
      "// alloy-http-start
      fn handler_http() {}
      // alloy-http-end

      // alloy-ws-start
      fn handler_ws() {}
      // alloy-ws-end

      fn handler() { real_impl() }
      "
    `);
  });

  it("re-emit with removed zone: strips orphaned sigil pair", () => {
    const existing = [
      "// alloy-http-start",
      "fn handler_http() {}",
      "// alloy-http-end",
      "// alloy-ws-start",
      "fn handler_ws() {}",
      "// alloy-ws-end",
      "",
      "fn handler() { real_impl() }",
    ].join("\n");

    // Only http in current zones, ws removed
    const result = emitWithExisting(existing, [
      { id: "http", content: "fn handler_http() {}" },
    ]);

    expect(result).toMatchInlineSnapshot(`
      "// alloy-http-start
      fn handler_http() {}
      // alloy-http-end

      fn handler() { real_impl() }
      "
    `);
  });

  it("re-emit with swapped zone: removes old, adds new", () => {
    const existing = [
      "// alloy-http-start",
      "fn handler_http() {}",
      "// alloy-http-end",
      "",
      "fn handler() { real_impl() }",
    ].join("\n");

    // Replace http with grpc
    const result = emitWithExisting(existing, [
      { id: "grpc", content: "fn handler_grpc() {}" },
    ]);

    expect(result).toMatchInlineSnapshot(`
      "// alloy-grpc-start
      fn handler_grpc() {}
      // alloy-grpc-end

      fn handler() { real_impl() }
      "
    `);
  });
});
