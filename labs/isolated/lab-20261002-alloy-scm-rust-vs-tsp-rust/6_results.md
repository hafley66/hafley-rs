# Assertion measurements

Primary: 41 PASS, 0 DIFF, 96 GAP, 137/137 inventoried.

All matchers: 111 PASS, 15 DIFF, 113 GAP, 239/239 inventoried.

Each row names the original A line. B is evaluated only from the original input. GAP rows have no candidate output. `6_results.json` retains oracle literals, input expressions, enclosing test bodies, parser contexts, and rustfmt output.

| A file:line | matcher | bucket | status | byte equality | strip-whitespace equality | reason | parse ERROR/MISSING | rustfmt exit |
|---|---|---|---|---|---|---|---|---|
| 00_name-policy.test.ts:10 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:14 | toBe | primary | PASS | true | true | none | 0/0 | 1 |
| 00_name-policy.test.ts:15 | toBe | primary | PASS | true | true | none | 0/0 | 1 |
| 00_name-policy.test.ts:19 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:23 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:27 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:33 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:34 | toBe | primary | PASS | true | true | none | 0/0 | 1 |
| 00_name-policy.test.ts:35 | toBe | primary | PASS | true | true | none | 0/0 | 1 |
| 00_name-policy.test.ts:36 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:40 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:41 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:45 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:46 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:50 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| 00_name-policy.test.ts:51 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| adapters/00_typespec-to-neutral.test.ts:54 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:109 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:137 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:174 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:202 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:236 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:299 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:360 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:395 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:402 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:431 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:472 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/00_typespec-to-neutral.test.ts:489 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/01_integration.test.tsx:105 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/01_integration.test.tsx:117 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/01_integration.test.tsx:135 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/01_integration.test.tsx:154 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/01_integration.test.tsx:167 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| adapters/01_integration.test.tsx:200 | toBe | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/0_primitives/2_Serde.test.tsx:21 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:26 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:31 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:36 | toBeNull | supplemental | PASS | n/a | n/a | none | n/a | n/a |
| components/0_primitives/2_Serde.test.tsx:40 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:47 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:51 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:55 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:60 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:64 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:69 | toBe | primary | PASS | true | true | none | 0/0 | 0 |
| components/0_primitives/2_Serde.test.tsx:74 | toBeNull | supplemental | PASS | n/a | n/a | none | n/a | n/a |
| components/0_primitives/2_Serde.test.tsx:80 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/0_primitives/2_Serde.test.tsx:98 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/0_primitives/2_Serde.test.tsx:120 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/0_primitives/2_Serde.test.tsx:139 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/0_primitives/2_Serde.test.tsx:156 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/0_primitives/2_Serde.test.tsx:173 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/0_primitives/2_Serde.test.tsx:192 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/0_primitives/2_Serde.test.tsx:212 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:19 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/0_StructDeclaration.test.tsx:25 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/0_StructDeclaration.test.tsx:31 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:49 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:65 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/0_StructDeclaration.test.tsx:71 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:88 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:104 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:120 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:136 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:156 | toRenderTo | supplemental | DIFF | false | false | generics | 0/0 | 1 |
| components/1_declarations/0_StructDeclaration.test.tsx:181 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/0_StructDeclaration.test.tsx:187 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/1_EnumDeclaration.test.tsx:25 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/1_EnumDeclaration.test.tsx:45 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/1_EnumDeclaration.test.tsx:63 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/1_EnumDeclaration.test.tsx:89 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/1_EnumDeclaration.test.tsx:117 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/1_EnumDeclaration.test.tsx:134 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/1_EnumDeclaration.test.tsx:152 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:21 | toRenderTo | supplemental | GAP | n/a | n/a | doc comments | n/a | n/a |
| components/1_declarations/2_parity.test.tsx:26 | toRenderTo | supplemental | GAP | n/a | n/a | doc comments | n/a | n/a |
| components/1_declarations/2_parity.test.tsx:31 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:38 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:43 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:48 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:53 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:58 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:63 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:68 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:73 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:78 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:83 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:90 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:95 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:100 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:108 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:121 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:126 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:131 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:137 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:149 | toRenderTo | supplemental | DIFF | false | true | whitespace/blank-line policy | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:164 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:176 | toRenderTo | supplemental | DIFF | false | false | generics | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:195 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:213 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:219 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:225 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:238 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:244 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 0 |
| components/1_declarations/2_parity.test.tsx:252 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:258 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:264 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:270 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/2_parity.test.tsx:278 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:19 | toRenderTo | supplemental | DIFF | false | true | whitespace/blank-line policy | 0/0 | 0 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:25 | toRenderTo | supplemental | DIFF | false | true | whitespace/blank-line policy | 0/0 | 0 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:39 | toRenderTo | supplemental | DIFF | false | true | whitespace/blank-line policy | 0/0 | 0 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:45 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:59 | toRenderTo | supplemental | DIFF | false | true | generics | 0/0 | 0 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:72 | toRenderTo | supplemental | DIFF | false | true | generics | 0/0 | 0 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:84 | toRenderTo | supplemental | DIFF | false | false | generics | 0/0 | 1 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:102 | toRenderTo | supplemental | DIFF | false | true | generics | 0/0 | 0 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:115 | toRenderTo | supplemental | DIFF | false | true | whitespace/blank-line policy | 0/0 | 0 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:121 | toRenderTo | supplemental | DIFF | false | true | whitespace/blank-line policy | 0/0 | 0 |
| components/1_declarations/4_FunctionDeclaration.test.tsx:127 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/6_ImplBlock.test.tsx:20 | toRenderTo | supplemental | DIFF | false | true | whitespace/blank-line policy | 0/0 | 0 |
| components/1_declarations/6_ImplBlock.test.tsx:26 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/6_ImplBlock.test.tsx:44 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/6_ImplBlock.test.tsx:69 | toRenderTo | supplemental | DIFF | false | true | generics | 0/0 | 0 |
| components/1_declarations/6_ImplBlock.test.tsx:75 | toRenderTo | supplemental | DIFF | false | false | generics | 0/0 | 0 |
| components/1_declarations/6_ImplBlock.test.tsx:92 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/1_declarations/6_ImplBlock.test.tsx:122 | toRenderTo | supplemental | PASS | true | true | none | 0/0 | 1 |
| components/2_references/0_Reference.test.tsx:39 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/2_references/0_Reference.test.tsx:66 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/2_references/0_Reference.test.tsx:100 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/2_references/0_Reference.test.tsx:146 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/3_files/0_SourceFile.test.tsx:37 | toBeNull | supplemental | PASS | n/a | n/a | none | n/a | n/a |
| components/3_files/0_SourceFile.test.tsx:38 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/3_files/0_SourceFile.test.tsx:67 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/3_files/0_SourceFile.test.tsx:97 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 0 |
| components/3_files/0_SourceFile.test.tsx:100 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:40 | toBeNull | supplemental | PASS | n/a | n/a | none | n/a | n/a |
| components/3_files/2_ModDirectory.test.tsx:41 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 0 |
| components/3_files/2_ModDirectory.test.tsx:66 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:95 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:128 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:139 | toBeNull | supplemental | PASS | n/a | n/a | none | n/a | n/a |
| components/3_files/2_ModDirectory.test.tsx:140 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 0 |
| components/3_files/2_ModDirectory.test.tsx:143 | toBeNull | supplemental | PASS | n/a | n/a | none | n/a | n/a |
| components/3_files/2_ModDirectory.test.tsx:144 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 0 |
| components/3_files/2_ModDirectory.test.tsx:162 | toMatchInlineSnapshot | primary | PASS | true | true | none | 0/0 | 0 |
| components/4_codegen/1_CodegenPair.test.tsx:54 | toBeNull | supplemental | GAP | n/a | n/a | refkey/import resolution | n/a | n/a |
| components/4_codegen/1_CodegenPair.test.tsx:55 | toContain | supplemental | GAP | n/a | n/a | refkey/import resolution | n/a | n/a |
| components/4_codegen/1_CodegenPair.test.tsx:58 | toBeNull | supplemental | GAP | n/a | n/a | refkey/import resolution | n/a | n/a |
| components/4_codegen/1_CodegenPair.test.tsx:59 | toContain | supplemental | GAP | n/a | n/a | refkey/import resolution | n/a | n/a |
| components/4_codegen/1_CodegenPair.test.tsx:84 | toContain | supplemental | GAP | n/a | n/a | refkey/import resolution | n/a | n/a |
| components/4_codegen/1_CodegenPair.test.tsx:109 | toContain | supplemental | GAP | n/a | n/a | refkey/import resolution | n/a | n/a |
| components/4_codegen/1_CodegenPair.test.tsx:142 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | refkey/import resolution | n/a | n/a |
| components/4_codegen/1_CodegenPair.test.tsx:152 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | refkey/import resolution | n/a | n/a |
| components/4_codegen/3_ReplaceFile.test.tsx:53 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/3_ReplaceFile.test.tsx:76 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/3_ReplaceFile.test.tsx:100 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/3_ReplaceFile.test.tsx:131 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/3_ReplaceFile.test.tsx:155 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/4_AxumEndpoint.test.tsx:101 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/4_AxumEndpoint.test.tsx:121 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/4_AxumEndpoint.test.tsx:143 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/6_Endpoint.test.tsx:75 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/6_Endpoint.test.tsx:109 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/6_Endpoint.test.tsx:146 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/6_Endpoint.test.tsx:188 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/6_Endpoint.test.tsx:233 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/4_codegen/6_Endpoint.test.tsx:285 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:46 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:74 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:106 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:131 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:161 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:233 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:248 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:252 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:262 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/0_integration.test.tsx:277 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/1_cargo-check.test.tsx:157 | toBe | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:130 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:149 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:161 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:179 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:193 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:210 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:216 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:222 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/2_axum-routing.test.tsx:252 | toBe | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/3_colocated-routing.test.tsx:137 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/3_colocated-routing.test.tsx:156 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/3_colocated-routing.test.tsx:171 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/3_colocated-routing.test.tsx:189 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/3_colocated-routing.test.tsx:203 | toBeNull | supplemental | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/3_colocated-routing.test.tsx:206 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/3_colocated-routing.test.tsx:222 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/3_colocated-routing.test.tsx:253 | toBe | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/4_auto-manual-split.test.tsx:162 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/4_auto-manual-split.test.tsx:176 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/4_auto-manual-split.test.tsx:188 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/4_auto-manual-split.test.tsx:203 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/4_auto-manual-split.test.tsx:216 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/4_auto-manual-split.test.tsx:236 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/4_auto-manual-split.test.tsx:268 | toBe | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/5_endpoint-component.test.tsx:134 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/5_endpoint-component.test.tsx:148 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/5_endpoint-component.test.tsx:160 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/5_endpoint-component.test.tsx:175 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/5_endpoint-component.test.tsx:188 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| components/5_tests/5_endpoint-component.test.tsx:230 | toBe | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/04_ops-plan.test.tsx:57 | toEqual | supplemental | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:28 | toEqual | supplemental | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:40 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:56 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:61 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:62 | toBe | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:64 | toContain | supplemental | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:65 | toContain | supplemental | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:85 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/06_emit-ops.test.tsx:93 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/07_daemon-files.test.tsx:13 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/07_daemon-files.test.tsx:15 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/07_daemon-files.test.tsx:41 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/07_daemon-files.test.tsx:59 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/07_daemon-files.test.tsx:60 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/emitter.test.tsx:74 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/emitter.test.tsx:91 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/emitter.test.tsx:108 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/emitter.test.tsx:115 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/emitter.test.tsx:121 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/emitter.test.tsx:158 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/emitter.test.tsx:197 | toMatchInlineSnapshot | primary | GAP | n/a | n/a | other | n/a | n/a |
| emitter/emitter.test.tsx:253 | toBe | primary | GAP | n/a | n/a | other | n/a | n/a |
| symbols/symbols.test.tsx:42 | toRenderTo | supplemental | GAP | n/a | n/a | scopes/symbol tables | n/a | n/a |
| symbols/symbols.test.tsx:59 | toRenderTo | supplemental | GAP | n/a | n/a | scopes/symbol tables | n/a | n/a |
| symbols/symbols.test.tsx:91 | toRenderTo | supplemental | GAP | n/a | n/a | scopes/symbol tables | n/a | n/a |
| symbols/symbols.test.tsx:105 | toBe | primary | GAP | n/a | n/a | scopes/symbol tables | n/a | n/a |
| symbols/symbols.test.tsx:107 | toBe | primary | GAP | n/a | n/a | scopes/symbol tables | n/a | n/a |
| symbols/symbols.test.tsx:111 | toRenderTo | supplemental | GAP | n/a | n/a | scopes/symbol tables | n/a | n/a |

## adapters/00_typespec-to-neutral.test.ts:54 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:109 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:137 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:174 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:202 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:236 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:299 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:360 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:395 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:402 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:431 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:472 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/00_typespec-to-neutral.test.ts:489 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/01_integration.test.tsx:105 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/01_integration.test.tsx:117 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/01_integration.test.tsx:135 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/01_integration.test.tsx:154 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/01_integration.test.tsx:167 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## adapters/01_integration.test.tsx:200 (GAP)

other: No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B

## components/1_declarations/0_StructDeclaration.test.tsx:156 (DIFF)

generics: expected 'struct Foo<T> where T: Serialize {\n …' to be 'struct Foo<T>\nwhere\n    T: Serializ…' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1,6 +1,3 @@
-struct Foo<T>
-where
-    T: Serialize,
- {
+struct Foo<T> where T: Serialize {
   bar: T,
 }
\ No newline at end of file
```

## components/1_declarations/2_parity.test.tsx:21 (GAP)

doc comments: Generated line_comment only exposes doc-marker fields; ordinary comment body is a pruned PATTERN

## components/1_declarations/2_parity.test.tsx:26 (GAP)

doc comments: Generated block_comment has no ordinary body prop

## components/1_declarations/2_parity.test.tsx:149 (DIFF)

whitespace/blank-line policy: expected 'trait Iterator {\n  type Item;\n\n\n …' to be 'trait Iterator {\n  type Item;\n  fn …' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1,4 +1,6 @@
 trait Iterator {
   type Item;
+
+
   fn next(&mut self) -> Option<Self::Item>;
 }
\ No newline at end of file
```

## components/1_declarations/2_parity.test.tsx:176 (DIFF)

generics: expected 'trait Store<T> where T: Serialize + S…' to be 'trait Store<T>\nwhere\n    T: Seriali…' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1,6 +1,3 @@
-trait Store<T>
-where
-    T: Serialize + Send,
- {
+trait Store<T> where T: Serialize + Send {
   fn save(&self, item: &T);
 }
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:19 (DIFF)

whitespace/blank-line policy: expected 'fn foo() {}' to be 'fn foo() { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-fn foo() { }
\ No newline at end of file
+fn foo() {}
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:25 (DIFF)

whitespace/blank-line policy: expected 'fn foo(bar: i32, baz: String) {}' to be 'fn foo(bar: i32, baz: String) { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-fn foo(bar: i32, baz: String) { }
\ No newline at end of file
+fn foo(bar: i32, baz: String) {}
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:39 (DIFF)

whitespace/blank-line policy: expected 'fn foo() -> i32 {}' to be 'fn foo() -> i32 { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-fn foo() -> i32 { }
\ No newline at end of file
+fn foo() -> i32 {}
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:59 (DIFF)

generics: expected 'fn foo<T>(bar: T) -> T {}' to be 'fn foo<T>(bar: T) -> T { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-fn foo<T>(bar: T) -> T { }
\ No newline at end of file
+fn foo<T>(bar: T) -> T {}
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:72 (DIFF)

generics: expected 'fn foo<T: Display>(bar: T) {}' to be 'fn foo<T: Display>(bar: T) { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-fn foo<T: Display>(bar: T) { }
\ No newline at end of file
+fn foo<T: Display>(bar: T) {}
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:84 (DIFF)

generics: expected 'fn foo<T>(bar: T) where T: Serialize …' to be 'fn foo<T>(bar: T)\nwhere\n    T: Seri…' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1,4 +1 @@
-fn foo<T>(bar: T)
-where
-    T: Serialize,
- { }
\ No newline at end of file
+fn foo<T>(bar: T) where T: Serialize {}
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:102 (DIFF)

generics: expected 'fn foo<\'a>(bar: &\'a str) -> &\'a st…' to be 'fn foo<\'a>(bar: &\'a str) -> &\'a st…' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-fn foo<'a>(bar: &'a str) -> &'a str { }
\ No newline at end of file
+fn foo<'a>(bar: &'a str) -> &'a str {}
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:115 (DIFF)

whitespace/blank-line policy: expected 'pub fn foo() {}' to be 'pub fn foo() { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-pub fn foo() { }
\ No newline at end of file
+pub fn foo() {}
\ No newline at end of file
```

## components/1_declarations/4_FunctionDeclaration.test.tsx:121 (DIFF)

whitespace/blank-line policy: expected 'async fn foo() {}' to be 'async fn foo() { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-async fn foo() { }
\ No newline at end of file
+async fn foo() {}
\ No newline at end of file
```

## components/1_declarations/6_ImplBlock.test.tsx:20 (DIFF)

whitespace/blank-line policy: expected 'impl Foo {}' to be 'impl Foo { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-impl Foo { }
\ No newline at end of file
+impl Foo {}
\ No newline at end of file
```

## components/1_declarations/6_ImplBlock.test.tsx:69 (DIFF)

generics: expected 'impl<T> Foo<T> {}' to be 'impl<T> Foo<T> { }' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1 +1 @@
-impl<T> Foo<T> { }
\ No newline at end of file
+impl<T> Foo<T> {}
\ No newline at end of file
```

## components/1_declarations/6_ImplBlock.test.tsx:75 (DIFF)

generics: expected 'impl<T> Foo<T> where T: Clone {}' to be 'impl<T> Foo<T>\nwhere\n    T: Clone,\…' // Object.is equality

```diff
--- A expected
+++ B actual
@@ -1,4 +1 @@
-impl<T> Foo<T>
-where
-    T: Clone,
- { }
\ No newline at end of file
+impl<T> Foo<T> where T: Clone {}
\ No newline at end of file
```

## components/4_codegen/1_CodegenPair.test.tsx:54 (GAP)

refkey/import resolution: The candidate has no paired-file context, impl delegation, or module synthesis

## components/4_codegen/1_CodegenPair.test.tsx:55 (GAP)

refkey/import resolution: The candidate has no paired-file context, impl delegation, or module synthesis

## components/4_codegen/1_CodegenPair.test.tsx:58 (GAP)

refkey/import resolution: The candidate has no paired-file context, impl delegation, or module synthesis

## components/4_codegen/1_CodegenPair.test.tsx:59 (GAP)

refkey/import resolution: The candidate has no paired-file context, impl delegation, or module synthesis

## components/4_codegen/1_CodegenPair.test.tsx:84 (GAP)

refkey/import resolution: The candidate has no paired-file context, impl delegation, or module synthesis

## components/4_codegen/1_CodegenPair.test.tsx:109 (GAP)

refkey/import resolution: The candidate has no paired-file context, impl delegation, or module synthesis

## components/4_codegen/1_CodegenPair.test.tsx:142 (GAP)

refkey/import resolution: The candidate has no paired-file context, impl delegation, or module synthesis

## components/4_codegen/1_CodegenPair.test.tsx:152 (GAP)

refkey/import resolution: The candidate has no paired-file context, impl delegation, or module synthesis

## components/4_codegen/3_ReplaceFile.test.tsx:53 (GAP)

other: The grammar printer has no existing-file zone splice or manual-content preservation API

## components/4_codegen/3_ReplaceFile.test.tsx:76 (GAP)

other: The grammar printer has no existing-file zone splice or manual-content preservation API

## components/4_codegen/3_ReplaceFile.test.tsx:100 (GAP)

other: The grammar printer has no existing-file zone splice or manual-content preservation API

## components/4_codegen/3_ReplaceFile.test.tsx:131 (GAP)

other: The grammar printer has no existing-file zone splice or manual-content preservation API

## components/4_codegen/3_ReplaceFile.test.tsx:155 (GAP)

other: The grammar printer has no existing-file zone splice or manual-content preservation API

## components/4_codegen/4_AxumEndpoint.test.tsx:101 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/4_codegen/4_AxumEndpoint.test.tsx:121 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/4_codegen/4_AxumEndpoint.test.tsx:143 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/4_codegen/6_Endpoint.test.tsx:75 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/4_codegen/6_Endpoint.test.tsx:109 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/4_codegen/6_Endpoint.test.tsx:146 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/4_codegen/6_Endpoint.test.tsx:188 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/4_codegen/6_Endpoint.test.tsx:233 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/4_codegen/6_Endpoint.test.tsx:285 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:46 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:74 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:106 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:131 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:161 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:233 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:248 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:252 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:262 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/0_integration.test.tsx:277 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/1_cargo-check.test.tsx:157 (GAP)

other: The A assertion depends on a whole generated scratch crate and Cargo check; B has no crate emitter

## components/5_tests/2_axum-routing.test.tsx:130 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/2_axum-routing.test.tsx:149 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/2_axum-routing.test.tsx:161 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/2_axum-routing.test.tsx:179 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/2_axum-routing.test.tsx:193 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/2_axum-routing.test.tsx:210 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/2_axum-routing.test.tsx:216 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/2_axum-routing.test.tsx:222 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/2_axum-routing.test.tsx:252 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/3_colocated-routing.test.tsx:137 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/3_colocated-routing.test.tsx:156 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/3_colocated-routing.test.tsx:171 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/3_colocated-routing.test.tsx:189 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/3_colocated-routing.test.tsx:203 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/3_colocated-routing.test.tsx:206 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/3_colocated-routing.test.tsx:222 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/3_colocated-routing.test.tsx:253 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/4_auto-manual-split.test.tsx:162 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/4_auto-manual-split.test.tsx:176 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/4_auto-manual-split.test.tsx:188 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/4_auto-manual-split.test.tsx:203 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/4_auto-manual-split.test.tsx:216 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/4_auto-manual-split.test.tsx:236 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/4_auto-manual-split.test.tsx:268 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/5_endpoint-component.test.tsx:134 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/5_endpoint-component.test.tsx:148 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/5_endpoint-component.test.tsx:160 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/5_endpoint-component.test.tsx:175 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/5_endpoint-component.test.tsx:188 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## components/5_tests/5_endpoint-component.test.tsx:230 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/04_ops-plan.test.tsx:57 (GAP)

other: No operation planning model or TypeSpec decorator registry in candidate B

## emitter/06_emit-ops.test.tsx:28 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/06_emit-ops.test.tsx:40 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/06_emit-ops.test.tsx:56 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/06_emit-ops.test.tsx:61 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/06_emit-ops.test.tsx:62 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/06_emit-ops.test.tsx:64 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/06_emit-ops.test.tsx:65 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/06_emit-ops.test.tsx:85 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/06_emit-ops.test.tsx:93 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/07_daemon-files.test.tsx:13 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/07_daemon-files.test.tsx:15 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/07_daemon-files.test.tsx:41 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/07_daemon-files.test.tsx:59 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/07_daemon-files.test.tsx:60 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/emitter.test.tsx:74 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/emitter.test.tsx:91 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/emitter.test.tsx:108 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/emitter.test.tsx:115 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/emitter.test.tsx:121 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/emitter.test.tsx:158 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/emitter.test.tsx:197 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## emitter/emitter.test.tsx:253 (GAP)

other: No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props

## symbols/symbols.test.tsx:42 (GAP)

scopes/symbol tables: The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata

## symbols/symbols.test.tsx:59 (GAP)

scopes/symbol tables: The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata

## symbols/symbols.test.tsx:91 (GAP)

scopes/symbol tables: The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata

## symbols/symbols.test.tsx:105 (GAP)

scopes/symbol tables: The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata

## symbols/symbols.test.tsx:107 (GAP)

scopes/symbol tables: The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata

## symbols/symbols.test.tsx:111 (GAP)

scopes/symbol tables: The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata

## All files from multi-file render calls

These checks also cover files with no A content assertion. Repeated file paths belong to separate test renders.

| A test file:line | B path | ERROR/MISSING | rustfmt exit |
|---|---|---|---|
| components/2_references/0_Reference.test.tsx:66 | models.rs | 0/0 | 0 |
| components/2_references/0_Reference.test.tsx:66 | lib.rs | 0/0 | 1 |
| components/3_files/0_SourceFile.test.tsx:37 | lib.rs | 0/0 | 1 |
| components/2_references/0_Reference.test.tsx:146 | models.rs | 0/0 | 0 |
| components/2_references/0_Reference.test.tsx:146 | errors.rs | 0/0 | 1 |
| components/2_references/0_Reference.test.tsx:146 | lib.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:66 | models/mod.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:66 | models/user.rs | 0/0 | 0 |
| components/3_files/2_ModDirectory.test.tsx:66 | lib.rs | 0/0 | 1 |
| components/3_files/0_SourceFile.test.tsx:67 | lib.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:95 | models/mod.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:95 | models/user.rs | 0/0 | 0 |
| components/3_files/2_ModDirectory.test.tsx:95 | lib.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:128 | a/mod.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:128 | a/b/mod.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:128 | a/b/thing.rs | 0/0 | 0 |
| components/3_files/2_ModDirectory.test.tsx:128 | lib.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:40 | models/mod.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:40 | models/user.rs | 0/0 | 0 |
| components/3_files/2_ModDirectory.test.tsx:40 | models/order.rs | 0/0 | 0 |
| components/3_files/2_ModDirectory.test.tsx:40 | lib.rs | 0/0 | 1 |
| components/2_references/0_Reference.test.tsx:39 | lib.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:162 | utils.rs | 0/0 | 1 |
| components/3_files/2_ModDirectory.test.tsx:162 | lib.rs | 0/0 | 1 |
| components/3_files/0_SourceFile.test.tsx:97 | models.rs | 0/0 | 0 |
| components/3_files/0_SourceFile.test.tsx:97 | lib.rs | 0/0 | 1 |
| components/2_references/0_Reference.test.tsx:100 | models.rs | 0/0 | 0 |
| components/2_references/0_Reference.test.tsx:100 | lib.rs | 0/0 | 1 |
