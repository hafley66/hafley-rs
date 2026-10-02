import { List, Output, render, refkey } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { SourceFile } from "../3_files/0_SourceFile.js";
import { ModDirectory } from "../3_files/2_ModDirectory.js";
import { StructDeclaration, StructField } from "../1_declarations/0_StructDeclaration.js";
import { FunctionDeclaration } from "../1_declarations/4_FunctionDeclaration.js";
import { EnumDeclaration, UnitVariant } from "../1_declarations/1_EnumDeclaration.js";
import { ImplBlock } from "../1_declarations/6_ImplBlock.js";

const OUTPUT_DIR = join(import.meta.dirname, "../../test-output");

function writeTree(node: any, dir: string) {
  for (const item of node.contents) {
    if (item.kind === "directory") {
      const subdir = join(dir, item.path);
      mkdirSync(subdir, { recursive: true });
      writeTree(item, dir);
    } else if (item.kind === "file") {
      const filePath = join(dir, item.path);
      mkdirSync(join(filePath, ".."), { recursive: true });
      writeFileSync(filePath, item.contents);
    }
  }
}

describe("cargo check", () => {
  it("generates a crate that compiles", () => {
    const userKey = refkey();
    const postKey = refkey();
    const appErrorKey = refkey();

    const res = render(
      <Output>
        <CrateDirectory>
          <ModDirectory name="models">
            <SourceFile path="user.rs"
              externalUses={["serde::Deserialize", "serde::Serialize"]}
            >
              <StructDeclaration
                name="User"
                refkey={userKey}
                pub
                derive={["Debug", "Clone", "Serialize", "Deserialize"]}
              >
                <List hardline>
                  <StructField name="id" type="i64" pub />
                  <StructField name="name" type="String" pub />
                  <StructField name="email" type="String" pub />
                </List>
              </StructDeclaration>
            </SourceFile>
            <SourceFile path="post.rs"
              externalUses={["serde::Deserialize", "serde::Serialize"]}
            >
              <StructDeclaration
                name="Post"
                refkey={postKey}
                pub
                derive={["Debug", "Clone", "Serialize", "Deserialize"]}
              >
                <List hardline>
                  <StructField name="id" type="i64" pub />
                  <StructField name="title" type="String" pub />
                  <StructField name="author_id" type="i64" pub />
                </List>
              </StructDeclaration>
            </SourceFile>
          </ModDirectory>
          <ModDirectory name="errors">
            <SourceFile path="error.rs">
              <EnumDeclaration name="AppError" refkey={appErrorKey} pub derive={["Debug"]}>
                <List hardline>
                  <UnitVariant name="NotFound" />
                  <UnitVariant name="InternalError" />
                </List>
              </EnumDeclaration>
              <hbr />
              <hbr />
              <ImplBlock target="AppError" trait="std::fmt::Display">
                <FunctionDeclaration
                  name="fmt"
                  selfParam="&"
                  params={[
                    { name: "f", type: "&mut std::fmt::Formatter<'_>" },
                  ]}
                  returns="std::fmt::Result"
                >
                  {"write!(f, \"{:?}\", self)"}
                </FunctionDeclaration>
              </ImplBlock>
            </SourceFile>
          </ModDirectory>
          <ModDirectory name="routes">
            <SourceFile path="users.rs">
              <FunctionDeclaration
                name="list_users"
                pub
                returns={<>Vec&lt;{userKey}&gt;</>}
              >
                vec![]
              </FunctionDeclaration>
              <hbr />
              <hbr />
              <FunctionDeclaration
                name="get_user"
                pub
                params={[{ name: "_id", type: "i64" }]}
                returns={<>Result&lt;{userKey}, {appErrorKey}&gt;</>}
              >
                Err(AppError::NotFound)
              </FunctionDeclaration>
            </SourceFile>
            <SourceFile path="posts.rs">
              <FunctionDeclaration
                name="list_posts"
                pub
                returns={<>Vec&lt;{postKey}&gt;</>}
              >
                vec![]
              </FunctionDeclaration>
            </SourceFile>
          </ModDirectory>
          <SourceFile path="lib.rs" />
        </CrateDirectory>
      </Output>
    );

    // Write to disk
    rmSync(OUTPUT_DIR, { recursive: true, force: true });
    mkdirSync(join(OUTPUT_DIR, "src"), { recursive: true });

    // Cargo.toml
    writeFileSync(join(OUTPUT_DIR, "Cargo.toml"), `[package]
name = "hafley-alloy-test-output"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
`);

    // Write generated source files into src/
    writeTree(res, join(OUTPUT_DIR, "src"));

    // cargo check
    try {
      const output = execSync("cargo check 2>&1", {
        cwd: OUTPUT_DIR,
        timeout: 120000,
        encoding: "utf-8",
      });
      // If we get here, it compiled
      expect(true).toBe(true);
    } catch (e: any) {
      // Print the compiler errors and fail
      console.error("cargo check failed:\n", e.stdout ?? e.stderr ?? e.message);
      expect.fail("cargo check failed -- see output above");
    }
  }, 120000);
});
