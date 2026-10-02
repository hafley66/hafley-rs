import { describe, expect, it } from "vitest";
import { namespaceToTypeDefs, programToTypeDefs } from "./00_typespec-to-neutral.js";

// Minimal mock builders that match the TypeSpec compiler type shapes.
// We only need the fields our adapter reads.

function mockScalar(name: string, baseScalar?: any): any {
  return { kind: "Scalar", name, baseScalar };
}

function mockModelProperty(name: string, type: any, optional = false): any {
  return { kind: "ModelProperty", name, type, optional };
}

function mockModel(name: string, properties: any[], indexer?: any): any {
  const propMap = new Map(properties.map((p: any) => [p.name, p]));
  return { kind: "Model", name, properties: propMap, indexer };
}

function mockEnum(name: string, members: { name: string; value?: string | number }[]): any {
  const memberMap = new Map(
    members.map(m => [m.name, { kind: "EnumMember", name: m.name, value: m.value, enum: null }]),
  );
  return { kind: "Enum", name, members: memberMap };
}

function mockNamespace(
  name: string,
  models: any[] = [],
  enums: any[] = [],
  namespaces: any[] = [],
): any {
  return {
    kind: "Namespace",
    name,
    models: new Map(models.map((m: any) => [m.name, m])),
    enums: new Map(enums.map((e: any) => [e.name, e])),
    namespaces: new Map(namespaces.map((ns: any) => [ns.name, ns])),
  };
}

describe("typespec-to-neutral", () => {
  it("converts a simple model with scalar properties", () => {
    const ns = mockNamespace("Api", [
      mockModel("User", [
        mockModelProperty("id", mockScalar("int64")),
        mockModelProperty("name", mockScalar("string")),
        mockModelProperty("email", mockScalar("string")),
        mockModelProperty("age", mockScalar("int32"), true),
      ]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs).toMatchInlineSnapshot(`
      [
        {
          "kind": "model",
          "name": "User",
          "properties": [
            {
              "name": "id",
              "optional": undefined,
              "type": {
                "kind": "scalar",
                "name": "int64",
              },
            },
            {
              "name": "name",
              "optional": undefined,
              "type": {
                "kind": "scalar",
                "name": "string",
              },
            },
            {
              "name": "email",
              "optional": undefined,
              "type": {
                "kind": "scalar",
                "name": "string",
              },
            },
            {
              "name": "age",
              "optional": true,
              "type": {
                "kind": "scalar",
                "name": "int32",
              },
            },
          ],
        },
      ]
    `);
  });

  it("resolves scalar base chains (uuid extends string)", () => {
    const stringScalar = mockScalar("string");
    const uuidScalar = mockScalar("uuid", stringScalar);

    const ns = mockNamespace("Api", [
      mockModel("Entity", [
        mockModelProperty("id", uuidScalar),
      ]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs[0]).toMatchInlineSnapshot(`
      {
        "kind": "model",
        "name": "Entity",
        "properties": [
          {
            "name": "id",
            "optional": undefined,
            "type": {
              "kind": "scalar",
              "name": "string",
            },
          },
        ],
      }
    `);
  });

  it("converts enums with values", () => {
    const ns = mockNamespace("Api", [], [
      mockEnum("Status", [
        { name: "Active", value: "active" },
        { name: "Inactive", value: "inactive" },
        { name: "Pending" },
      ]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs).toMatchInlineSnapshot(`
      [
        {
          "kind": "enum",
          "members": [
            {
              "name": "Active",
              "value": "active",
            },
            {
              "name": "Inactive",
              "value": "inactive",
            },
            {
              "name": "Pending",
              "value": undefined,
            },
          ],
          "name": "Status",
        },
      ]
    `);
  });

  it("converts array properties (Model named Array with indexer)", () => {
    const arrayModel = mockModel("Array", [], {
      key: mockScalar("integer"),
      value: mockScalar("string"),
    });

    const ns = mockNamespace("Api", [
      mockModel("TaggedItem", [
        mockModelProperty("tags", arrayModel),
      ]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs[0].kind === "model" && defs[0].properties[0]).toMatchInlineSnapshot(`
      {
        "name": "tags",
        "optional": undefined,
        "type": {
          "element": {
            "kind": "scalar",
            "name": "string",
          },
          "kind": "array",
        },
      }
    `);
  });

  it("converts Record properties (Model named Record with indexer)", () => {
    const recordModel = mockModel("Record", [], {
      key: mockScalar("string"),
      value: mockScalar("int32"),
    });

    const ns = mockNamespace("Api", [
      mockModel("Scores", [
        mockModelProperty("values", recordModel),
      ]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs[0].kind === "model" && defs[0].properties[0]).toMatchInlineSnapshot(`
      {
        "name": "values",
        "optional": undefined,
        "type": {
          "key": {
            "kind": "scalar",
            "name": "string",
          },
          "kind": "map",
          "value": {
            "kind": "scalar",
            "name": "int32",
          },
        },
      }
    `);
  });

  it("converts model-to-model references", () => {
    const address = mockModel("Address", [
      mockModelProperty("street", mockScalar("string")),
      mockModelProperty("city", mockScalar("string")),
    ]);

    const ns = mockNamespace("Api", [
      address,
      mockModel("User", [
        mockModelProperty("name", mockScalar("string")),
        mockModelProperty("address", address),
      ]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs).toMatchInlineSnapshot(`
      [
        {
          "kind": "model",
          "name": "Address",
          "properties": [
            {
              "name": "street",
              "optional": undefined,
              "type": {
                "kind": "scalar",
                "name": "string",
              },
            },
            {
              "name": "city",
              "optional": undefined,
              "type": {
                "kind": "scalar",
                "name": "string",
              },
            },
          ],
        },
        {
          "kind": "model",
          "name": "User",
          "properties": [
            {
              "name": "name",
              "optional": undefined,
              "type": {
                "kind": "scalar",
                "name": "string",
              },
            },
            {
              "name": "address",
              "optional": undefined,
              "type": {
                "kind": "model",
                "name": "Address",
              },
            },
          ],
        },
      ]
    `);
  });

  it("converts enum references in model properties", () => {
    const statusEnum: any = { kind: "Enum", name: "Status", members: new Map() };

    const ns = mockNamespace("Api", [
      mockModel("Order", [
        mockModelProperty("id", mockScalar("int64")),
        mockModelProperty("status", statusEnum),
      ]),
    ], [
      mockEnum("Status", [{ name: "Pending" }, { name: "Shipped" }]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs).toMatchInlineSnapshot(`
      [
        {
          "kind": "model",
          "name": "Order",
          "properties": [
            {
              "name": "id",
              "optional": undefined,
              "type": {
                "kind": "scalar",
                "name": "int64",
              },
            },
            {
              "name": "status",
              "optional": undefined,
              "type": {
                "kind": "enum",
                "name": "Status",
              },
            },
          ],
        },
        {
          "kind": "enum",
          "members": [
            {
              "name": "Pending",
              "value": undefined,
            },
            {
              "name": "Shipped",
              "value": undefined,
            },
          ],
          "name": "Status",
        },
      ]
    `);
  });

  it("converts nested array of models", () => {
    const user = mockModel("User", [
      mockModelProperty("name", mockScalar("string")),
    ]);

    const arrayOfUsers = mockModel("Array", [], {
      key: mockScalar("integer"),
      value: user,
    });

    const ns = mockNamespace("Api", [
      user,
      mockModel("Team", [
        mockModelProperty("members", arrayOfUsers),
      ]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    const teamDef = defs.find(d => d.name === "Team");
    expect(teamDef).toMatchInlineSnapshot(`
      {
        "kind": "model",
        "name": "Team",
        "properties": [
          {
            "name": "members",
            "optional": undefined,
            "type": {
              "element": {
                "kind": "model",
                "name": "User",
              },
              "kind": "array",
            },
          },
        ],
      }
    `);
  });

  it("recurses into sub-namespaces when recursive: true", () => {
    const childNs = mockNamespace("Sub", [
      mockModel("Inner", [
        mockModelProperty("val", mockScalar("string")),
      ]),
    ]);

    const rootNs = mockNamespace("Root", [
      mockModel("Outer", [
        mockModelProperty("id", mockScalar("int64")),
      ]),
    ], [], [childNs]);

    const nonRecursive = namespaceToTypeDefs(rootNs);
    expect(nonRecursive.map(d => d.name)).toMatchInlineSnapshot(`
      [
        "Outer",
      ]
    `);

    const recursive = namespaceToTypeDefs(rootNs, { recursive: true });
    expect(recursive.map(d => d.name)).toMatchInlineSnapshot(`
      [
        "Outer",
        "Inner",
      ]
    `);
  });

  it("programToTypeDefs skips TypeSpec stdlib namespace", () => {
    const stdlibNs = mockNamespace("TypeSpec", [
      mockModel("Array", []),
      mockModel("Record", []),
    ]);

    const userNs = mockNamespace("MyService", [
      mockModel("User", [
        mockModelProperty("name", mockScalar("string")),
      ]),
    ], [
      mockEnum("Role", [{ name: "Admin" }, { name: "Member" }]),
    ]);

    const globalNs = mockNamespace("", [], [], [stdlibNs, userNs]);

    const program = {
      getGlobalNamespaceType: () => globalNs,
    };

    const defs = programToTypeDefs(program);
    expect(defs).toMatchInlineSnapshot(`
      [
        {
          "kind": "model",
          "name": "User",
          "properties": [
            {
              "name": "name",
              "optional": undefined,
              "type": {
                "kind": "scalar",
                "name": "string",
              },
            },
          ],
        },
        {
          "kind": "enum",
          "members": [
            {
              "name": "Admin",
              "value": undefined,
            },
            {
              "name": "Member",
              "value": undefined,
            },
          ],
          "name": "Role",
        },
      ]
    `);
  });

  it("skips anonymous models (empty name)", () => {
    const ns = mockNamespace("Api", [
      mockModel("", [mockModelProperty("x", mockScalar("string"))]),
      mockModel("Real", [mockModelProperty("y", mockScalar("int32"))]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs.map(d => d.name)).toMatchInlineSnapshot(`
      [
        "Real",
      ]
    `);
  });

  it("handles unknown type kinds by falling back to string scalar", () => {
    const unionType = { kind: "Union", name: "Foo" };

    const ns = mockNamespace("Api", [
      mockModel("Thing", [
        mockModelProperty("data", unionType),
      ]),
    ]);

    const defs = namespaceToTypeDefs(ns);
    expect(defs[0].kind === "model" && defs[0].properties[0].type).toMatchInlineSnapshot(`
      {
        "kind": "scalar",
        "name": "string",
      }
    `);
  });
});
