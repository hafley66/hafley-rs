import {
  List,
  Output,
  refkey,
  render,
} from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { VisibilityContext } from "../../scopes/06_contexts.js";
import { StructDeclaration, StructField } from "../1_declarations/0_StructDeclaration.js";
import { SourceFile } from "../3_files/0_SourceFile.js";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { ZoneProvider, Zone } from "../4_codegen/0_AppendZone.js";
import { FunctionDeclaration } from "../1_declarations/4_FunctionDeclaration.js";
import { Endpoint } from "./6_Endpoint.js";
import type { ClassifiedParam } from "./5_TransportBinding.js";

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

describe("Endpoint", () => {
  const userKey = refkey();

  function buildTree(endpoints: any) {
    return render(
      <Output>
        <VisibilityContext.Provider value="pub">
          <ZoneProvider>
            <CrateDirectory>
              <SourceFile path="user.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                <StructDeclaration name="User" refkey={userKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                  <List hardline>
                    <StructField name="id" type="i64" />
                    <StructField name="name" type="String" />
                  </List>
                </StructDeclaration>
              </SourceFile>

              {endpoints}

              <SourceFile path="lib.rs" externalUses={["axum::Router", "axum::routing::get"]}>
                <FunctionDeclaration name="router" returns="Router">
                  Router::new()<Zone name="router" />
                </FunctionDeclaration>
                <FunctionDeclaration name="ws_router" returns="WsRouter">
                  WsRouter::new()<Zone name="ws_router" />
                </FunctionDeclaration>
              </SourceFile>
            </CrateDirectory>
          </ZoneProvider>
        </VisibilityContext.Provider>
      </Output>
    );
  }

  it("HTTP-only, no params", () => {
    const res = buildTree(
      <Endpoint
        name="list_users"
        path="/users"
        method="get"
        routePath="list_users"
        responseModel={userKey}
        responseList
      />
    );
    const file = findFile(res, "list_users.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;

      use crate::user::User;

      // alloy-http-start
      pub async fn list_users_http() -> Json<Vec<User>> {
        Json(list_users().await)
      }
      // alloy-http-end

      pub async fn list_users() -> Vec<User> {
        todo!()
      }"
    `);
  });

  it("HTTP-only with sourced params", () => {
    const params: ClassifiedParam[] = [
      { kind: "sourced", source: "path", name: "org_id", rustType: "i64" },
      { kind: "sourced", source: "query", name: "limit", rustType: "i32" },
    ];
    const res = buildTree(
      <Endpoint
        name="list_org_users"
        path="/orgs/:org_id/users"
        method="get"
        routePath="list_org_users"
        responseModel={userKey}
        responseList
        params={params}
      />
    );
    const file = findFile(res, "list_org_users.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::Path;
      use axum::extract::Query;

      use crate::user::User;

      // alloy-http-start
      pub async fn list_org_users_http(Path(org_id): Path<i64>, Query(limit): Query<i32>) -> Json<Vec<User>> {
        Json(list_org_users(org_id, limit).await)
      }
      // alloy-http-end

      pub async fn list_org_users(org_id: i64, limit: i32) -> Vec<User> {
        let _ = org_id;
        let _ = limit;
        todo!()
      }"
    `);
  });

  it("HTTP + WS dual binding", () => {
    const params: ClassifiedParam[] = [
      { kind: "sourced", source: "path", name: "room_id", rustType: "uuid::Uuid" },
    ];
    const res = buildTree(
      <Endpoint
        name="get_room"
        path="/rooms/:room_id"
        method="get"
        routePath="get_room"
        responseModel={userKey}
        transports={["http", "ws"]}
        params={params}
      />
    );
    const file = findFile(res, "get_room.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::Path;

      use crate::user::User;

      // alloy-http-start
      pub async fn get_room_http(Path(room_id): Path<uuid::Uuid>) -> Json<User> {
        Json(get_room(room_id).await)
      }
      // alloy-http-end
      // alloy-ws-start
      pub async fn get_room_ws(msg: GetRoomMsg, conn: &WsConnection) -> WsResponse {
        WsResponse::json(get_room(msg.room_id).await)
      }
      // alloy-ws-end

      pub async fn get_room(room_id: uuid::Uuid) -> User {
        let _ = room_id;
        todo!()
      }"
    `);
  });

  it("resolved PerRequest param: bare in HTTP, extracted in WS", () => {
    const sessionKey = refkey();
    const params: ClassifiedParam[] = [
      { kind: "resolved", scope: "per_request", name: "session", rustType: "UserSession", innerTypeRef: sessionKey },
      { kind: "sourced", source: "path", name: "room_id", rustType: "uuid::Uuid" },
    ];
    const res = buildTree(
      <Endpoint
        name="join_room"
        path="/rooms/:room_id"
        method="get"
        routePath="join_room"
        responseModel={userKey}
        transports={["http", "ws"]}
        params={params}
      />
    );
    const file = findFile(res, "join_room.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Extension;
      use axum::Json;
      use axum::extract::Path;

      use crate::user::User;

      // alloy-http-start
      pub async fn join_room_http(Extension(session): Extension<UserSession>, Path(room_id): Path<uuid::Uuid>) -> Json<User> {
        Json(join_room(session, room_id).await)
      }
      // alloy-http-end
      // alloy-ws-start
      pub async fn join_room_ws(msg: JoinRoomMsg, conn: &WsConnection) -> WsResponse {
        let session = conn.extensions().get::<UserSession>().cloned().unwrap();
        WsResponse::json(join_room(session, msg.room_id).await)
      }
      // alloy-ws-end

      pub async fn join_room(session: UserSession, room_id: uuid::Uuid) -> User {
        let _ = session;
        let _ = room_id;
        todo!()
      }"
    `);
  });

  it("resolved Shared param: State<T> in HTTP, conn.state() in WS", () => {
    const storeKey = refkey();
    const params: ClassifiedParam[] = [
      { kind: "resolved", scope: "shared", name: "rooms", rustType: "RoomStore", innerTypeRef: storeKey },
      { kind: "sourced", source: "body", name: "stroke", rustType: "Stroke" },
    ];
    const res = buildTree(
      <Endpoint
        name="draw_stroke"
        path="/rooms/:room_id/draw"
        method="post"
        routePath="draw_stroke"
        responseModel={userKey}
        transports={["http", "ws"]}
        params={params}
      />
    );
    const file = findFile(res, "draw_stroke.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::State;

      use crate::user::User;

      // alloy-http-start
      pub async fn draw_stroke_http(State(rooms): State<RoomStore>, Json(stroke): Json<Stroke>) -> Json<User> {
        Json(draw_stroke(rooms, stroke).await)
      }
      // alloy-http-end
      // alloy-ws-start
      pub async fn draw_stroke_ws(msg: DrawStrokeMsg, conn: &WsConnection) -> WsResponse {
        let rooms = conn.state::<RoomStore>();
        WsResponse::json(draw_stroke(rooms, msg.stroke).await)
      }
      // alloy-ws-end

      pub async fn draw_stroke(rooms: RoomStore, stroke: Stroke) -> User {
        let _ = rooms;
        let _ = stroke;
        todo!()
      }"
    `);
  });

  it("router collects registrations per transport", () => {
    const params: ClassifiedParam[] = [
      { kind: "sourced", source: "path", name: "room_id", rustType: "uuid::Uuid" },
    ];
    const res = buildTree(
      <>
        <Endpoint
          name="list_users"
          path="/users"
          method="get"
          routePath="list_users"
          responseModel={userKey}
          responseList
        />
        <Endpoint
          name="join_room"
          path="/rooms/:room_id"
          method="get"
          routePath="join_room"
          responseModel={userKey}
          transports={["http", "ws"]}
          params={params}
        />
      </>
    );
    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod join_room;
      pub mod list_users;
      pub mod user;

      use axum::Router;
      use axum::routing::get;

      pub fn router() -> Router {
        Router::new()
        .route("/users", get(list_users::list_users_http))
        .route("/rooms/:room_id", get(join_room::join_room_http))
      }pub fn ws_router() -> WsRouter {
        WsRouter::new()
        .on("join_room", join_room::join_room_ws)
      }"
    `);
  });
});
