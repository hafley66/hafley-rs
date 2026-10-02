// B twin of components/4_codegen/3_ReplaceFile.test.tsx; original inputs and oracle literals are in fixtures/ and 3_assertions.json.
import { it } from "vitest";
import { gap } from "../../../4_probe.js";
it("\"first emit: builds zones + manual content\" assertion 1", () => gap("905b2bc1da68dc45", "other", "The grammar printer has no existing-file zone splice or manual-content preservation API"));
it("\"re-emit same zones: replaces auto content, preserves manual\" assertion 2", () => gap("0ca385312982ad83", "other", "The grammar printer has no existing-file zone splice or manual-content preservation API"));
it("\"re-emit with added zone: appends new zone before manual content\" assertion 3", () => gap("5cdac7fa2c2ec7a0", "other", "The grammar printer has no existing-file zone splice or manual-content preservation API"));
it("\"re-emit with removed zone: strips orphaned sigil pair\" assertion 4", () => gap("dedf65a0eec2c128", "other", "The grammar printer has no existing-file zone splice or manual-content preservation API"));
it("\"re-emit with swapped zone: removes old, adds new\" assertion 5", () => gap("58315087e49b8740", "other", "The grammar printer has no existing-file zone splice or manual-content preservation API"));
