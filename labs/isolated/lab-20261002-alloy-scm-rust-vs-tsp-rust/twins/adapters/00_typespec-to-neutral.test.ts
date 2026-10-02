// B twin of adapters/00_typespec-to-neutral.test.ts; original inputs and oracle literals are in fixtures/ and 3_assertions.json.
import { it } from "vitest";
import { gap } from "../../4_probe.js";
it("\"converts a simple model with scalar properties\" assertion 1", () => gap("ecd60bf4c8e4747a", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"resolves scalar base chains (uuid extends string)\" assertion 2", () => gap("c40d72e77b171b55", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"converts enums with values\" assertion 3", () => gap("5ecd95149727b255", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"converts array properties (Model named Array with indexer)\" assertion 4", () => gap("eb08194b25e4d15c", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"converts Record properties (Model named Record with indexer)\" assertion 5", () => gap("2770dec56fd47d85", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"converts model-to-model references\" assertion 6", () => gap("9e0ffc70fd41e767", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"converts enum references in model properties\" assertion 7", () => gap("4563a72bfead3f7d", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"converts nested array of models\" assertion 8", () => gap("d2e3ea8fed84de3c", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"recurses into sub-namespaces when recursive: true\" assertion 9", () => gap("aca4205bdaf23dee", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"recurses into sub-namespaces when recursive: true\" assertion 10", () => gap("41b22ec0316c3b68", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"programToTypeDefs skips TypeSpec stdlib namespace\" assertion 11", () => gap("df9bdf1f63c16a13", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"skips anonymous models (empty name)\" assertion 12", () => gap("da09e1a327793e55", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
it("\"handles unknown type kinds by falling back to string scalar\" assertion 13", () => gap("c74c8d2399a819b4", "other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"));
