import { describe, expect, it } from "vitest";
import { createRustNamePolicy, RustElements } from "./00_name-policy.js";

const policy = createRustNamePolicy();
const n = (name: string, element: RustElements) => policy.getName(name, element);

describe("name-policy", () => {
  describe("passes names through unchanged", () => {
    it("structs", () => {
      expect(n("MyCoolStruct", "struct")).toBe("MyCoolStruct");
    });

    it("fields", () => {
      expect(n("first_name", "field")).toBe("first_name");
      expect(n("firstName", "field")).toBe("firstName");
    });

    it("functions", () => {
      expect(n("get_user_by_id", "function")).toBe("get_user_by_id");
    });

    it("constants", () => {
      expect(n("MAX_CONNECTIONS", "constant")).toBe("MAX_CONNECTIONS");
    });

    it("modules", () => {
      expect(n("my_module", "module")).toBe("my_module");
    });
  });

  describe("keyword escaping", () => {
    it("escapes strict keywords with r#", () => {
      expect(n("type", "function")).toBe("r#type");
      expect(n("match", "variable")).toBe("r#match");
      expect(n("self", "parameter")).toBe("r#self");
      expect(n("struct", "module")).toBe("r#struct");
    });

    it("escapes reserved keywords", () => {
      expect(n("try", "function")).toBe("r#try");
      expect(n("yield", "function")).toBe("r#yield");
    });

    it("does not escape lifetimes even if they collide", () => {
      expect(n("a", "lifetime")).toBe("a");
      expect(n("static", "lifetime")).toBe("static");
    });

    it("does not escape non-keywords", () => {
      expect(n("foo", "function")).toBe("foo");
      expect(n("MyStruct", "struct")).toBe("MyStruct");
    });
  });
});
