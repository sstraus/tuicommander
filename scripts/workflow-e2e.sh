#!/usr/bin/env bash
# Real-daemon workflow evidence. No mock or fixture execution path.
set -euo pipefail
exec python3 -B "$(dirname "$0")/workflow-e2e/bootstrap.py" "$@"
