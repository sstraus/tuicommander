# Recorded remote dual-stream envelopes

Recorded on 2026-11-01 from the unmodified `REMOTE_COMMAND` at commit
`c7ab606a862289f50c05f24687acbca39f4e3b51` on macOS with Python 3.
These are actual producer stdout bytes, gzip-compressed without rewriting JSON.
The portable client test decompresses them in a child and captures its stdout.

Recording recipe: extract the raw `REMOTE_COMMAND` string from `executor.rs`,
then execute `[sys.executable, "-c", runner, request, "--", sys.executable, "-c", script]`
with `request = '{"names":[],"timeout":10}'` and each script below.
Require successful exit and empty stderr, then save `gzip.compress(stdout, mtime=0)`.

## remote-dual-stream-controls.txt.gz

Producer script: `import sys; sys.stdout.write(chr(1)*600000); sys.stderr.write(chr(2)*600000)`

Uncompressed bytes: 983311.
Uncompressed SHA-256: `3b59760975566c0250ad7fb285b0446dc4b8940379ac3ddb93a79c93e7add9d8`.

## remote-dual-stream-ascii.txt.gz

Producer script: `import sys; sys.stdout.write('a'*600000); sys.stderr.write('b'*600000)`

Uncompressed bytes: 983321.
Uncompressed SHA-256: `bf607fb4cef374e98497f9b2c5efe4929c1b0178e4d140c94ecb0f822b0ec618`.

## remote-dual-stream-unicode.txt.gz

Producer script: `import sys; sys.stdout.write(chr(0x1f600)*600000); sys.stderr.write(chr(0x1f601)*600000)`

Uncompressed bytes: 983299.
Uncompressed SHA-256: `b3fec77c1102254dc5c501b71eba95f62298a085de7eef93464393ee8f875516`.
