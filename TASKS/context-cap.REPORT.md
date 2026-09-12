# context-cap.REPORT.md

Why a Claude Code session on `claude-fable-5-1` shows a 200k context window, and the one-line change for 1M. Binary inspected: `~/.local/share/claude/versions/2.1.269` (unpatched). Offsets are byte offsets in that file.

## Cause table

| Candidate | Grep receipt | Verdict |
|---|---|---|
| Baked catalog sets fable-5-1 to 200k | None. `id:"claude-fable-5-1"` at 161981279, `context:{...}` at 161981776 = `context:{window:1e6,native_1m:!0,supports_1m_beta:!0}`. Baked table says 1M, not 200k | Not the baked table |
| Served/published catalog overrides window | No literal in binary (fetched at runtime, not statically grep-able). `providerCache` in `OV()` at 162401976 holds `servedCatalog`, `publishedCatalog`, `gatewayModelsStorageV5`. These override the baked window at runtime | Likely the 200k source for the bare model |
| `[1m]` suffix unsupported for fable | `claude-fable-5-1[1m]` literal: 0 hits. `claude-opus-5[1m]`: 2 hits. `fable[1m]`: 6 hits (alias table `GD` at 161968373). Suffix handled generically, not per-model | False. Suffix works (see live test) |
| Default fallback window is 200k | `jSe=200000` at 162633990, `F$=200000` at 162634001 | The 200k denominator source when no 1M path fires |
| `kc(e)` forces 1M on any `[1m]` model string | `function kc(e){if(V0())return!1;return xf(e)}function xf(e){return/\[1m\]/i.test(e)}` at 162634085. `qz`: `if(kc(e))return 1e6` at 162636374 | 1M override path, model-agnostic |
| `CLAUDE_CODE_MAX_CONTEXT_TOKENS` forces window | `CLAUDE_CODE_MAX_CONTEXT_TOKENS` refs at 162635593, 162635918; env list at 160843947. `Yz()`/`qz()` gate it on `DISABLE_COMPACT` and `$z(e)` | Alternative 1M path, gated |

## Runtime path for `claude-fable-5-1`

`qz(e,n)` at 162636374:
- `if(kc(e)) return 1e6` (model string contains `[1m]`).
- else `SFn(e)` reads the model catalog `context.window` via `ul()`. A served catalog entry for fable-5-1 reporting `window:200000` and no `native_1m` yields `believed:200000`.
- else returns `jSe` = 200000.

The baked table says fable-5-1 is native 1M (`native_1m:!0`), so a bare `claude-fable-5-1` should resolve 1M unless the served catalog downgrades it to 200k. `[1m]` bypasses the catalog entirely through `kc(e)`.

## Live proof

`claude --model 'claude-fable-5-1[1m]' -p 'say ok' --output-format json` from `/tmp/ctxprobe`:

```
"modelUsage":{"claude-fable-5-1[1m]":{"contextWindow":1000000,"canonicalModel":"claude-fable-5-1",...}}
```

The `[1m]` suffix resolves to `canonicalModel claude-fable-5-1` with `contextWindow 1000000`. Opus parity: transcript rows attach `claude-opus-5[1m]` the same way.

## Statusline

`~/.claude/statusline.sh` prints `.context_window.context_window_size / 1000` as the denominator (`CONTEXT_SIZE/1000`, then `${CONTEXT_SIZE_K}k`). It renders what Claude Code reports; it is not the cap. 200000 prints `200k`, 1000000 prints `1000k`.

## One-line change

`~/.claude/settings.json:18`, add the `[1m]` suffix:

```diff
-  "model": "claude-fable-5-1",
+  "model": "claude-fable-5-1[1m]",
```

The suffix survives model resolution (`canonicalModel claude-fable-5-1`) and forces `contextWindow 1000000` through `kc(e)`/`qz`. Alternative with same effect: `CLAUDE_CODE_MAX_CONTEXT_TOKENS=1000000` (gated on `DISABLE_COMPACT` and model recognizability, less direct).
