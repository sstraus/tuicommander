# Producer captures

Raw stdout of the real mdkb CLI (not edited): `mdkb 3.14.0`, `target/debug/mdkb` sha256 `86187796b319d720bcb5411cb3f576415f55479aae95a474aebf67ee0110dbf8`, captured 2026-11-15.

Isolated store: throwaway git repo (2 markdown files, 1 Rust file) in a temp dir outside `~/Gits`; env `MDKB_NO_DAEMON=1`, temp `HOME`/`USERPROFILE`, `FASTEMBED_CACHE_DIR` = copy of cached MiniLM/Granite snapshots, `HF_ENDPOINT=http://127.0.0.1:9`, `HF_HOME` unset. Temp store deleted after capture.

1. `mdkb init && mdkb update` (default MiniLM) then `mdkb --format json stats` -> `stats-current.json`
2. `.mdkb/config.toml` set to `[embedding] model = "granite107m"`, then `mdkb --format json stats` -> `stats-stale.json` (documents and code both stale)
3. `mdkb --format json embed --reembed` -> `embed-reembed.json` (exit 0)
4. `mdkb --format json stats` -> `stats-after-reembed.json`

Not captured: `pending_model`, `ambiguous`, non-empty `errors` (covered only by domain-input tests).
