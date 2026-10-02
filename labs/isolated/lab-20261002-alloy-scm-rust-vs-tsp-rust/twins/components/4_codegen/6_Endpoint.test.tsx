// B twin of components/4_codegen/6_Endpoint.test.tsx; original inputs and oracle literals are in fixtures/ and 3_assertions.json.
import { it } from "vitest";
import { gap } from "../../../4_probe.js";
it("\"HTTP-only, no params\" assertion 1", () => gap("d5afe4322bc28ef0", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"HTTP-only with sourced params\" assertion 2", () => gap("e523805afb8c3f1f", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"HTTP + WS dual binding\" assertion 3", () => gap("3851d1aa388e19dd", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"resolved PerRequest param: bare in HTTP, extracted in WS\" assertion 4", () => gap("6a266770c61d77fc", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"resolved Shared param: State<T> in HTTP, conn.state() in WS\" assertion 5", () => gap("e52daa144e65c08b", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"router collects registrations per transport\" assertion 6", () => gap("db8d398c006b2960", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
