# An opencode model switch stalls turn attribution

Session `ses_f8265b8cdffe5AsLH5pH54EqT5` (154 messages at capture, `deepseek/deepseek-v4-flash-0731`
switched to `z-ai/glm-5.3-flash`). Attribution broke at message
`msg_07db9686c0019Yr3z400eqj0I6` (rowid 59767): an assistant row the switched-to
model wrote with zero parts and a terminal 429 (`[DeepInfra] z-ai/glm-5.3-flash is
temporarily rate-limited upstream`). boop's cursor sat at 59766 while opencode's
last rowid was 59993, so 66 later messages never projected and the pane's turn
marks ended at turn 189. Same stall in `ses_f82e957aeffeexQYLISXk08eMo` at
`msg_07d1a0255001paKVy9CFa5o2sP` (aborted assistant, zero parts), cursor 59406.

The wrong call: `messages_after` in `crates/boop-harness/src/harness/opencode.rs`
broke its row walk on `!has_parts`, reading "no parts yet" as "assistant still
streaming, retry next sync". A terminal partless assistant message never gains
parts, so every retry broke on the same row and the cursor never moved again.

| message rowid | role | model | parts | terminal | projected turn |
| --- | --- | --- | --- | --- | --- |
| 59766 | assistant | glm-5.3-flash | 3 | done | 189 (last ever) |
| 59767 | assistant | glm-5.3-flash | 0 | 429 error | none, walk breaks here |
| 59768 | user | - | 1 | - | missing |
| 59769 | assistant | glm-5.3-flash | 5 | done | missing |
| 59993 | assistant | glm-5.3-flash | 4 | done | missing (last row at capture) |

Fix: a terminal assistant message with no parts is skipped (`continue`), so the
walk advances and later messages project with their own roles and turn indices;
the error row itself holds no readable content and projects nothing. Covered by
`a_model_switch_survives_a_partless_errored_assistant_message`.
