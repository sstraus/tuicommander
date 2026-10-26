# Tool search catalogue capture (2300-6355)

Captured on 2026-10-24 from the running TUICommander instance. The coordinator authorized the read-only local IPC socket after the TCP endpoint returned HTTP 401.

```sh
curl -fsS --unix-socket "$HOME/Library/Application Support/com.tuic.commander/mcp.sock" \
  -H 'Content-Type: application/json' \
  -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":2300,"method":"tools/list","params":{}}' \
  http://localhost/mcp
```

The response contained 199 tools in 213,713 bytes. Its SHA-256 was `2519d8240d0b9d3d97a59acb94fdddbdcaac5f9f8a9a6aa2d4715fe58b9a9d39`.

`tool-search-2300.json` retains the exact tool objects for native `repo`, `session`, and `agent`, plus all six `quill__` tools. Only the response envelope and unrelated tools were removed; JSON was pretty-printed. No name, description, or schema was invented or edited. The reduced fixture SHA-256 is `7f8279bb5e98bd6cc296141580958345ac29ea505a9cc43b12ae00543d76174d`.

This subset preserves the description-only `repo` match in `quill__get_file` and the competing callable native tool. Alias-prefix unit cases are explicit synthetic name variants, not claimed live server output.
