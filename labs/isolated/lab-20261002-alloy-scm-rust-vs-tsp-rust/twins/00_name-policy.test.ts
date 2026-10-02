import { probe, probeTest } from "../4_probe.js";
import { describe, expect, it } from "vitest";
import { createRustNamePolicy, RustElements } from "../2_components.js";

const policy = createRustNamePolicy();
const n = (name: string, element: RustElements) => policy.getName(name, element);

describe("name-policy", () => {
  describe("passes names through unchanged", () => {
    it("structs", () => probeTest(["3b0f94dfad40fa3c"], () => {
      probe("3b0f94dfad40fa3c", () => (n("MyCoolStruct", "struct")), "MyCoolStruct");
    }));

    it("fields", () => probeTest(["18fa59686dd364b9","a50ddd96d6f4c6ca"], () => {
      probe("18fa59686dd364b9", () => (n("first_name", "field")), "first_name");
      probe("a50ddd96d6f4c6ca", () => (n("firstName", "field")), "firstName");
    }));

    it("functions", () => probeTest(["a55f9cd5c7d7e217"], () => {
      probe("a55f9cd5c7d7e217", () => (n("get_user_by_id", "function")), "get_user_by_id");
    }));

    it("constants", () => probeTest(["12b1a2b2e6587c96"], () => {
      probe("12b1a2b2e6587c96", () => (n("MAX_CONNECTIONS", "constant")), "MAX_CONNECTIONS");
    }));

    it("modules", () => probeTest(["7bef9b5512c2efeb"], () => {
      probe("7bef9b5512c2efeb", () => (n("my_module", "module")), "my_module");
    }));
  });

  describe("keyword escaping", () => {
    it("escapes strict keywords with r#", () => probeTest(["222f1475bb85b25b","95b69fb0891c9b27","2c1f8313e839e3dc","ae019ca5e8afa830"], () => {
      probe("222f1475bb85b25b", () => (n("type", "function")), "r#type");
      probe("95b69fb0891c9b27", () => (n("match", "variable")), "r#match");
      probe("2c1f8313e839e3dc", () => (n("self", "parameter")), "r#self");
      probe("ae019ca5e8afa830", () => (n("struct", "module")), "r#struct");
    }));

    it("escapes reserved keywords", () => probeTest(["7fae3702a15a8bff","a66ab51a25e30cb5"], () => {
      probe("7fae3702a15a8bff", () => (n("try", "function")), "r#try");
      probe("a66ab51a25e30cb5", () => (n("yield", "function")), "r#yield");
    }));

    it("does not escape lifetimes even if they collide", () => probeTest(["55ee00fd5a7dc41d","c73345a6a039d861"], () => {
      probe("55ee00fd5a7dc41d", () => (n("a", "lifetime")), "a");
      probe("c73345a6a039d861", () => (n("static", "lifetime")), "static");
    }));

    it("does not escape non-keywords", () => probeTest(["3dd8d20c5604078e","f2a3ea50021bc665"], () => {
      probe("3dd8d20c5604078e", () => (n("foo", "function")), "foo");
      probe("f2a3ea50021bc665", () => (n("MyStruct", "struct")), "MyStruct");
    }));
  });
});
