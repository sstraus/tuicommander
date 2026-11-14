#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec node --test --test-concurrency=1 "$root/scripts/story-adapter-conformance/protocol.test.mjs"
