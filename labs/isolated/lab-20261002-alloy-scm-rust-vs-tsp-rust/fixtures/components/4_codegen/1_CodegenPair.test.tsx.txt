import { Output, Scope, createScope, render } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../scopes/01_lexical.js";
import { FunctionDeclaration } from "../1_declarations/4_FunctionDeclaration.js";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { SourceFile } from "../3_files/0_SourceFile.js";
import { CodegenPair, AutoFile, StubFile, ImplCall } from "./1_CodegenPair.js";

function RustRoot(props: { children: any }) {
  const scope = createScope(RustLexicalScope, "root", undefined);
  return (
    <Output>
      <Scope value={scope}>
        {props.children}
      </Scope>
    </Output>
  );
}

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

describe("CodegenPair", () => {
  it("creates paired auto + stub files with correct names", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <CodegenPair name="process_event" implPath="crate::process_event">
            <AutoFile name="process_event">
              <FunctionDeclaration name="process_event" pub>
                <ImplCall fn="process_event_impl" args={["event"]} />.await
              </FunctionDeclaration>
            </AutoFile>
            <StubFile name="process_event">
              <FunctionDeclaration name="process_event_impl" pub>
                todo!()
              </FunctionDeclaration>
            </StubFile>
          </CodegenPair>
          <SourceFile path="lib.rs" />
        </CrateDirectory>
      </Output>
    );

    const autoFile = findFile(res, "process_event_auto.rs");
    expect(autoFile).not.toBeNull();
    expect(autoFile.contents).toContain("crate::process_event::process_event_impl(event)");

    const stubFile = findFile(res, "process_event.rs");
    expect(stubFile).not.toBeNull();
    expect(stubFile.contents).toContain("todo!()");
  });

  it("ImplCall renders delegation path from context", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <CodegenPair name="handle_msg" implPath="crate::handle_msg">
            <AutoFile name="handle_msg">
              <FunctionDeclaration name="handle_msg" pub>
                <ImplCall fn="handle_msg_impl" />.await
              </FunctionDeclaration>
            </AutoFile>
            <StubFile name="handle_msg">
              <FunctionDeclaration name="handle_msg_impl" pub>
                todo!()
              </FunctionDeclaration>
            </StubFile>
          </CodegenPair>
          <SourceFile path="lib.rs" />
        </CrateDirectory>
      </Output>
    );

    const autoFile = findFile(res, "handle_msg_auto.rs");
    expect(autoFile.contents).toContain("crate::handle_msg::handle_msg_impl()");
  });

  it("ImplCall with no args renders empty parens", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <CodegenPair name="tick" implPath="crate::systems::tick">
            <AutoFile name="tick">
              <FunctionDeclaration name="tick" pub>
                <ImplCall fn="tick_impl" />
              </FunctionDeclaration>
            </AutoFile>
            <StubFile name="tick">
              <FunctionDeclaration name="tick_impl" pub>
                todo!()
              </FunctionDeclaration>
            </StubFile>
          </CodegenPair>
          <SourceFile path="lib.rs" />
        </CrateDirectory>
      </Output>
    );

    const autoFile = findFile(res, "tick_auto.rs");
    expect(autoFile.contents).toContain("crate::systems::tick::tick_impl()");
  });

  it("works for non-HTTP use case (event handler)", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <CodegenPair name="on_click" implPath="crate::on_click">
            <AutoFile name="on_click" externalUses={["events::Event", "events::EventContext"]}>
              <FunctionDeclaration
                name="on_click"
                pub
                params={[{ name: "ctx", type: "EventContext" }, { name: "event", type: "Event" }]}
              >
                <ImplCall fn="on_click_impl" args={["ctx", "event"]} />
              </FunctionDeclaration>
            </AutoFile>
            <StubFile name="on_click">
              <FunctionDeclaration
                name="on_click_impl"
                pub
                params={[{ name: "ctx", type: "EventContext" }, { name: "event", type: "Event" }]}
              >
                let _ = (ctx, event);{"\n"}todo!()
              </FunctionDeclaration>
            </StubFile>
          </CodegenPair>
          <SourceFile path="lib.rs" />
        </CrateDirectory>
      </Output>
    );

    const autoFile = findFile(res, "on_click_auto.rs");
    expect(autoFile.contents.trim()).toMatchInlineSnapshot(`
      "use events::Event;
      use events::EventContext;

      pub fn on_click(ctx: EventContext, event: Event) {
        crate::on_click::on_click_impl(ctx, event)
      }"
    `);

    const stubFile = findFile(res, "on_click.rs");
    expect(stubFile.contents.trim()).toMatchInlineSnapshot(`
      "pub fn on_click_impl(ctx: EventContext, event: Event) {
        let _ = (ctx, event);
        todo!()
      }"
    `);
  });
});
