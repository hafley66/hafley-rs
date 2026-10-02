// B twin of components/5_tests/3_colocated-routing.test.tsx; original inputs and oracle literals are in fixtures/ and 3_assertions.json.
import { it } from "vitest";
import { gap } from "../../../4_probe.js";
it("\"lib.rs: flat mod declarations + router\" assertion 1", () => gap("aab4aae73bb2712a", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"user.rs at crate root (hoisted)\" assertion 2", () => gap("5eedfd8d22dce7a8", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"post.rs colocated in orgs/, references User from parent\" assertion 3", () => gap("7ffa39b1969d4316", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"users.rs handler at root, refs sibling user module\" assertion 4", () => gap("4520c432f549102e", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"orgs/posts.rs handler refs colocated Post\" assertion 5", () => gap("f0335dbe3f2b7a8a", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"orgs/posts.rs handler refs colocated Post\" assertion 6", () => gap("8f8b2f99b6202700", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"orgs/mod.rs declares colocated children\" assertion 7", () => gap("6b76cf20ab0a6a78", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
it("\"compiles with cargo check, no unused vars\" assertion 8", () => gap("6de3a47e0c415b73", "other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"));
