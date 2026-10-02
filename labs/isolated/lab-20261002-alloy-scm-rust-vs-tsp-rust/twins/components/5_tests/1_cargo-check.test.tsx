// B twin of components/5_tests/1_cargo-check.test.tsx; original inputs and oracle literals are in fixtures/ and 3_assertions.json.
import { it } from "vitest";
import { gap } from "../../../4_probe.js";
it("\"generates a crate that compiles\" assertion 1", () => gap("3f26422c17bf982b", "other", "The A assertion depends on a whole generated scratch crate and Cargo check; B has no crate emitter"));
