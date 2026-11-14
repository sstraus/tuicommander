#!/bin/sh
# Fake 1Password CLI for provider tests. Behaviour is chosen by the reference
# (last argument). Never talks to a real vault. Invoked as `sh fake-op.sh ...`.
for last; do :; done
case "$last" in
  op://v/i/ok) printf 's3cr3t-value-2914' ;;
  op://v/i/args) printf '%s' "$*" ;;
  op://v/i/empty) ;;
  op://v/i/missing)
    echo '[ERROR] 2026/11/08 10:00:00 "missing" isn'"'"'t an item in the "v" vault.' >&2; exit 1 ;;
  op://v/i/signedout)
    echo "[ERROR] 2026/11/08 10:00:00 You are not currently signed in. Please run 'op signin'." >&2; exit 1 ;;
  op://v/i/locked)
    echo '[ERROR] 2026/11/08 10:00:00 authorization prompt dismissed, please try again' >&2; exit 1 ;;
  op://v/i/hang) exec sleep 30 ;;
  op://v/i/daemon) sleep 8 & printf 's3cr3t-value-2914' ;;
  *) echo '[ERROR] 2026/11/08 10:00:00 something unexpected happened' >&2; exit 1 ;;
esac
