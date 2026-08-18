# boop host chat

`boop host chat` reads one JSON request from stdin and writes one JSON row to stdout.

```json
{"resident":"review","model":"gpt-5.6","goal":"Review each request.","prompt":"Inspect this change."}
```

The first request for a resident may include `goal`. Later requests reopen that resident's harness conversation from `~/.agent/run/<resident>/chat.json`. A successful response is `{"reply_turn":7,"reply":"..."}`. Operational failures print `{"outcome":"failed","detail":"..."}` and exit 0.
