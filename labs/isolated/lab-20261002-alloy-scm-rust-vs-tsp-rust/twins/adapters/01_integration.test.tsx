// B twin of adapters/01_integration.test.tsx; original inputs and oracle literals are in fixtures/ and 3_assertions.json.
import { it } from "vitest";
import { gap } from "../../4_probe.js";
it("\"full pipeline: TypeSpec mock -> neutral types -> Rust crate\" assertion 1", () => gap("98718a8a05716a85", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"full pipeline: TypeSpec mock -> neutral types -> Rust crate\" assertion 2", () => gap("f0d73112e10b9c36", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"full pipeline: TypeSpec mock -> neutral types -> Rust crate\" assertion 3", () => gap("621453809df70342", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"full pipeline: TypeSpec mock -> neutral types -> Rust crate\" assertion 4", () => gap("cedfa7ede1bfb8fc", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"full pipeline: TypeSpec mock -> neutral types -> Rust crate\" assertion 5", () => gap("3febb38c4894efbb", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"compiles with cargo check\" assertion 6", () => gap("1edf66bdf0b29cda", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
