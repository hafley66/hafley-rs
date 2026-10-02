// B twin of symbols/symbols.test.tsx; original inputs and oracle literals are in fixtures/ and 3_assertions.json.
import { it } from "vitest";
import { gap } from "../../4_probe.js";
it("\"creates a struct symbol via factory\" assertion 1", () => gap("efe044e5a1ecd22f", "scopes/symbol tables", "The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata"));
it("\"creates a function symbol via factory\" assertion 2", () => gap("aaf273af7a7bda4c", "scopes/symbol tables", "The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata"));
it("\"creates field symbols inside a named type scope\" assertion 3", () => gap("045c7e38f46011f3", "scopes/symbol tables", "The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata"));
it("\"RustNamedTypeSymbol tracks typeKind reactively\" assertion 4", () => gap("4794d3da29706e99", "scopes/symbol tables", "The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata"));
it("\"RustNamedTypeSymbol tracks typeKind reactively\" assertion 5", () => gap("a61b8a39cd1ede5a", "scopes/symbol tables", "The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata"));
it("\"RustNamedTypeSymbol tracks typeKind reactively\" assertion 6", () => gap("824c6a0e0f351346", "scopes/symbol tables", "The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata"));
