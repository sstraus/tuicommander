#!/usr/bin/env bash
# Machine-wide build admission. Runs a build command when the machine has room
# for it, instead of agents polling a gate in model turns.
#
# Usage: tools/build/build-slot.sh <cmd> [args ...]          (run it with run_in_background)
#        tools/build/build-slot.sh --gate <cmd> [args ...]   coverage, CRAP, mutation
#        tools/build/build-slot.sh --remote --short <cmd>    on the box, a job under 10 min: reserved slot, killed at 10 min
#        RB_EXPERIMENTAL_MBX=1 tools/build/build-slot.sh --remote <cmd>  experimental box shim
#        tools/build/build-slot.sh status
#
# Admission is dynamic. With no admitted build, a build starts when free memory
# is >= 15%. Builds 2 and 3 climb a ladder set in build-slot.conf (free memory %,
# load1 ceiling, a memory surcharge per running agent; see read_conf below).
# Evidence for the memory bar: 11:51 on 2026-09-26 one 12 GB rustc left 0.4% free
# memory and froze the TUIC UI; a lone rustc spends most of its time on one core.
# The decision and the registration happen under one lock, so two agents cannot
# both take the last slot. The build runs at nice 5.
#
#
# Gates (--gate) never delay a normal build (Boss, 2026-09-26: mutation and
# coverage runs must not block a RED/GREEN check). They do not take a normal slot,
# at most BUILD_SLOT_GATES run at a time (default 2, Boss 2026-09-27: with one, five
# night gates queued for hours behind a single baseline), and one starts only when no normal build runs or waits. It
# runs at nice 19. Not in the macOS background band (taskpolicy -b): its threads got 37 s
# CPU in 15 min at load 45, and one holding an mbx flock starved 13 builds for 26 min (2026-10-03).
# While a normal build runs or waits, the gate's whole process tree (a parent-link walk, so
# children that left the group are included) is stopped (SIGSTOP) and resumed
# after. A gate stopped for BUILD_SLOT_PREEMPT seconds (default 120) is killed
# and exits 75 (TERM to the tree, KILL after BUILD_SLOT_GRACE seconds): a stopped gate can hold a lock the build needs, so the stop is
# bounded. Re-run a gate that exits 75. When PREEMPT is longer than
# BUILD_SLOT_STALL (default 300), as at night, the gate is instead resumed after
# STALL seconds and runs beside the build until no build runs: a stopped gate that
# owns the mbx actions a build waits for deadlocked both for 78 min (2026-09-27).
#
# Tests override the probes: BUILD_SLOT_DIR, BUILD_SLOT_FREE, BUILD_SLOT_LOAD,
# BUILD_SLOT_POLL, BUILD_SLOT_PREEMPT, BUILD_SLOT_STALL, BUILD_SLOT_GRACE, BUILD_SLOT_GATES (tools/tests/build-slot-gate.test.sh).
# The whole body is one compound command, so bash parses it before running it:
# rewriting this file then cannot derail running wrappers (2026-09-26, two
# wrappers read the new file at their old byte offset and held slots for an hour).
{
set -uo pipefail
# mbx picks each worktree's target dir; an inherited CARGO_TARGET_DIR overrides it. TUIC
# passes its dev value (tuicommander/src-tauri/target) to every PTY, so all repos built in
# one target, waited on its lock and ran stale binaries (2026-09-27).
unset CARGO_TARGET_DIR
remote=0 remote_gate=0 remote_short=0
for (( option = 0; option < 2; option++ )); do
  case ${1:-} in
    --remote) remote=1; shift ;;
    --gate) remote_gate=1; shift ;;
    --short) remote_short=1; shift ;;
    *) break ;;
  esac
done
# An option is never a build command: "--help" once queued behind real builds (2026-09-29).
if [[ ${1:-} == -* ]]; then
  sed -n '/^# Usage:/,/^#$/p' "$0" | sed 's/^# \{0,1\}//'
  [[ $1 == -h || $1 == --help ]] && exit 0
  exit 2
fi
# The tuicommander main checkout's target holds the binaries of the TUIC Boss runs. A build there
# replaced them twice: dictation lost its microphone permission (2026-09-30), and macOS dropped
# every non-loopback flow, killing mobile access (tuic-bump's cargo check, 2026-10-04, story
# 1510-03ae). Local builds run in a worktree; --remote builds run on the box.
protected=${BUILD_SLOT_PROTECTED:-$HOME/Gits/personal/tuicommander}
if (( ! remote )) && [[ $(git rev-parse --show-toplevel 2>/dev/null) == "$protected" ]]; then
  echo "build-slot: REFUSED: $protected is a protected checkout (the running TUIC's binaries); build in a worktree or with --remote" >&2
  exit 2
fi
# Canonical path: tuic-land finds running rb jobs by "<repo>/tools/remote/rb" in the command
# line; "tools/build/../remote/rb" hid two live jobs from it on 2026-09-29.
rb_bin=${RB_BIN:-$(realpath "$(dirname "$(realpath "$0")")/../remote/rb")}
if (( remote )); then
  # rb snapshots the directory it is given: from src-tauri/ it dropped repo-root include_str!
  # inputs and the remote build failed (2026-09-29). cd inside the command instead.
  top=$(git rev-parse --show-toplevel 2>/dev/null) || top=$PWD
  if [[ $(cd "$top" && pwd -P) != "$(pwd -P)" ]]; then
    echo "build-slot: --remote runs from the repository root ($top); put 'cd <subdir>' inside the command" >&2
    exit 2
  fi
  # Mutation and coverage runs are class gate (Boss, 2026-09-30): as normal or short they hold a
  # normal slot for hours and starve every build.
  if (( ! remote_gate )) && [[ " $* " =~ (cargo[[:space:]]+(mutants|llvm-cov)|stryker|gremlins|muter)([[:space:]]|$) ]]; then
    echo "build-slot: '${BASH_REMATCH[1]}' is a mutation/coverage run: submit it with --remote --gate" >&2
    exit 2
  fi
  export RB_CLASS=$([[ $remote_gate == 1 ]] && echo gate || { [[ $remote_short == 1 ]] && echo short || echo normal; })
  cache_option=
  case ${RB_EXPERIMENTAL_MBX:-0} in
    0) ;;
    1) cache_option=--experimental-mbx ;;
    *) echo 'build-slot: RB_EXPERIMENTAL_MBX must be 0 or 1' >&2; exit 2 ;;
  esac
  # Decide before starting rb: never cancel a running remote command and replay
  # it locally (the command may already have observable effects). Pinned refs and
  # the experimental Linux cache cannot be replayed in the current Mac checkout.
  if (( ! remote_gate )) && [[ -z ${RB_REF:-} && -z $cache_option && -z ${RB_HANDLE_FILE:-}${RB_STDIN_FILE:-}${RB_SECRET_ENV_FILE:-} && $top != "$protected" ]]; then
    overload=$(python3 - "$rb_bin" "$RB_CLASS" <<'PY_OVERLOAD'
import re
import subprocess
import sys
try:
    result = subprocess.run([sys.argv[1], 'status'], capture_output=True, text=True, timeout=10)
    if result.returncode in (69, 255):
        print(f'remote unavailable (status exit {result.returncode})')
    elif result.returncode == 0:
        for line in result.stdout.splitlines():
            # Ignore gates: their long waits do not prove story-build overload.
            if not (f'class={sys.argv[2]} ' in line or line.startswith(f'remote admission {sys.argv[2]}:')):
                continue
            memory = re.search(r'memory free ([0-9]+)% < ([0-9]+)%', line)
            waited = re.search(r'waited=([0-9]+)s', line)
            if memory or (waited and int(waited[1]) > 120):
                print(line.strip())
                break
except subprocess.TimeoutExpired:
    print('remote status timed out before execution')
except OSError as error:
    print(f'remote status unavailable before execution: {error}')
except subprocess.SubprocessError:
    pass  # Other status errors do not prove an outage.
PY_OVERLOAD
)
    if [[ -n $overload ]]; then
      local_dir=${BUILD_SLOT_DIR:-$HOME/Gits/.tmp/build-slot}
      # status uses the same probes as admission; an explicit dir avoids its rb
      # status recursion. The real local admission below rechecks under its lock.
      local_status=$(BUILD_SLOT_DIR="$local_dir" "$0" status 2>/dev/null) || local_status=
      if [[ $local_status =~ ^running=0[[:space:]]waiting=0[[:space:]]gates=[0-9]+[[:space:]]free=([0-9]+)%[[:space:]]load1=([0-9]+)[[:space:]]disk=([0-9]+)GiB ]] &&
        (( BASH_REMATCH[1] >= 15 && BASH_REMATCH[3] >= 25 )) &&
        [[ ! -f $local_dir/BUILD_FREEZE && ! -f $HOME/Gits/.tmp/BUILD_FREEZE ]]; then
        echo "build-slot: rb preflight ($overload); local slot free, running locally" >&2
        # No --remote: protected checkout, freeze, memory and slot checks all
        # still apply, and the mbx shim remains the only local cargo entry point.
        exec "$0" "$@"
      fi
    fi
  fi
  # A gate can mix Rust and frontend commands (e.g. make cov). Prove the
  # runner's store for every pnpm-lock.yaml gate, inside the same rb snapshot.
  # Send the helper as command text: target repositories need not contain it.
  # No local fallback is allowed for gate admission, including this proof.
  if (( remote_gate )); then
    frontend_proof=$(cat "$(dirname "$(realpath "$0")")/rb-frontend-ready.py") || exit 69
    set -- python3 -c "$frontend_proof" "$@"
  fi
  if [[ -n ${RB_REF:-} ]]; then
    exec "$rb_bin" ${cache_option:+"$cache_option"} --ref "$RB_REF" "$PWD" -- "$@"
  fi
  if [[ ${RB_FORCE:-0} == 1 ]]; then
    exec "$rb_bin" ${cache_option:+"$cache_option"} --force "$PWD" -- "$@"
  fi
  exec "$rb_bin" ${cache_option:+"$cache_option"} "$PWD" -- "$@"
fi
(( remote_gate )) && set -- --gate "$@"
# Plain cargo builds into a worktree-local target/, outside the mbx cache: 16 worktrees held
# ~50 GB and every build started cold (2026-09-28). Put the mbx shim first for every command.
export PATH="${BUILD_SLOT_MBX_BIN:-$HOME/Library/Application Support/mbx/bin}:$PATH"
dir="${BUILD_SLOT_DIR:-$HOME/Gits/.tmp/build-slot}"
poll="${BUILD_SLOT_POLL:-20}"
preempt="${BUILD_SLOT_PREEMPT:-120}"
stall="${BUILD_SLOT_STALL:-300}"
grace="${BUILD_SLOT_GRACE:-10}"
# Admission ladder (Boss, 2026-10-03: up to 3 concurrent builds, tuned live). Thresholds live in
# build-slot.conf, re-read on every poll so tuning reaches queued builds. A lone build keeps the
# 15% rule; build k+1 (k running) needs free >= SLOT<k+1>_FREE + agents above AGENT_BASE times
# AGENT_FREE_PCT, and load1 <= SLOT<k+1>_LOAD. Agents = lines of every coordinator children.tsv.
. "$(dirname "$(realpath "$0")")/../coord-state.sh"
conf="${BUILD_SLOT_CONF:-$(dirname "$(realpath "$0")")/build-slot.conf}"
read_conf() {
  SLOT_MAX=1 SLOT2_FREE=50 SLOT2_LOAD=20 SLOT3_FREE=65 SLOT3_LOAD=14 AGENT_BASE=6 AGENT_FREE_PCT=1
  # shellcheck source=/dev/null
  [[ -f $conf ]] && . "$conf"
  max=$SLOT_MAX
}
agents() { [[ -n ${BUILD_SLOT_AGENTS:-} ]] && { echo "$BUILD_SLOT_AGENTS"; return; }
  coord_children_all | grep -c . || true; }
# need_free <running> and need_load <running>: the bar for the next build.
need_free() { (( $1 == 0 )) && { echo 15; return; }
  local base extra a; a=$(agents); extra=$(( a > AGENT_BASE ? (a - AGENT_BASE) * AGENT_FREE_PCT : 0 ))
  case $1 in 1) base=$SLOT2_FREE ;; *) base=$SLOT3_FREE ;; esac
  echo $(( base + extra )); }
need_load() { case $1 in 0) echo 999 ;; 1) echo "$SLOT2_LOAD" ;; *) echo "$SLOT3_LOAD" ;; esac; }
read_conf
max_gates="${BUILD_SLOT_GATES:-2}"
mkdir -p "$dir/running" "$dir/waiting" "$dir/gates"
# BUILD_FREEZE (tools/build/build-freeze.sh) holds every admission, not just agents that read it:
# on 2026-09-29 a queued build started seconds after Boss froze compilation for an hour.
# BUILD_SLOT_AGENT names the caller for the flag's "allowed:" list.
freeze_file="$HOME/Gits/.tmp/BUILD_FREEZE"
[[ -n ${BUILD_SLOT_DIR:-} ]] && freeze_file="$dir/BUILD_FREEZE"
frozen() {
  [[ -f $freeze_file ]] || return 1
  local allowed; allowed=$(sed -n 's/^allowed: //p' "$freeze_file")
  [[ -n ${BUILD_SLOT_AGENT:-} && " $allowed " == *" $BUILD_SLOT_AGENT "* ]] && return 1
  return 0
}

free_pct() { [[ -n ${BUILD_SLOT_FREE:-} ]] && { echo "$BUILD_SLOT_FREE"; return; }
  memory_pressure | tail -1 | grep -Eo '[0-9]+' | tail -1; }
# Free GiB on the volume that holds ~/Gits: ~20 worktree builds filled it to 119 MiB
# on 2026-09-28 and every tool then failed with ENOSPC. Builds wait below 25 GiB.
disk_gib() { [[ -n ${BUILD_SLOT_DISK_GIB:-} ]] && { echo "$BUILD_SLOT_DISK_GIB"; return; }
  df -k "$HOME/Gits" | awk 'NR==2{print int($4/1048576)}'; }
# mbx keeps rustc incremental state under <cache>/incremental with no size or age
# limit: it reached 53 GiB on 2026-09-28, held disk under 25 GiB and stalled every
# agent build for 30 minutes. It is rebuildable, so below the floor prune entries
# idle for 2 hours (active builds touch theirs).
reclaim_incremental() {
  local inc="${BUILD_SLOT_INCREMENTAL:-$HOME/Gits/.mbx/incremental}"
  [[ -d $inc ]] || return 0
  find "$inc" -mindepth 3 -maxdepth 3 -type d -mmin +120 -exec rm -rf {} + 2>/dev/null
  echo "build-slot: disk under 25 GiB, pruned idle mbx incremental state" >&2
}
load1() { [[ -n ${BUILD_SLOT_LOAD:-} ]] && { echo "$BUILD_SLOT_LOAD"; return; }
  sysctl -n vm.loadavg | awk '{print int($2)}'; }

# Count live entries in a registry (running, waiting, gates); drop dead owners.
count() {
  local n=0 f
  for f in "$dir/$1"/*; do
    [[ -e $f ]] || continue
    if kill -0 "${f##*/}" 2>/dev/null; then n=$((n + 1)); else rm -f "$f"; fi
  done
  echo "$n"
}
running() { count running; }
busy() { (( $(count running) + $(count waiting) > 0 )); }

# Every pid under $1, parents first. cargo-nextest and cargo-mutants put their children in
# their own process groups, so a group signal missed the compilers (2026-10-03); the parent
# link finds them wherever their group is.
descendants() {
  ps -axo pid=,ppid= | awk -v root="$1" '
    { kids[$2] = kids[$2] " " $1 }
    END { queue[1] = root; head = 1; tail = 1
      while (head <= tail) {
        n = split(kids[queue[head]], k, " ")
        for (i = 1; i <= n; i++) { queue[++tail] = k[i]; print k[i] }
        head++ } }'
}
# signal_tree <signal> <root>: the root's group plus every descendant. Two passes, so a
# process forked between the walk and the signal is caught by the second.
signal_tree() {
  local pass
  for pass in 1 2; do
    kill -"$1" -- -"$2" "$2" $(descendants "$2") 2>/dev/null
  done
}
live() { local s; s=$(ps -o stat= -p "$1" 2>/dev/null); [[ -n $s && $s != Z* ]]; }
# end_tree <root>: TERM, resume, wait up to $grace s, then KILL what is left. The wait is
# bounded: a command that traps or survives TERM must not keep the wrapper alive.
end_tree() {
  local pids p i alive=""
  pids="$1 $(descendants "$1")"
  kill -TERM -- -"$1" $pids 2>/dev/null; kill -CONT -- -"$1" $pids 2>/dev/null
  for (( i = 0; i < grace; i++ )); do
    alive=""; for p in $pids; do live "$p" && alive+=" $p"; done
    [[ -z $alive ]] && break
    sleep 1
  done
  [[ -n $alive ]] && kill -KILL -- -"$1" $alive 2>/dev/null
  return 0
}

lock() {
  until mkdir "$dir/lock" 2>/dev/null; do
    local owner; owner=$(cat "$dir/lock/pid" 2>/dev/null)
    if [[ -n $owner ]]; then
      kill -0 "$owner" 2>/dev/null || rm -rf "$dir/lock"
    elif (( $(date +%s) - $(stat -f %m "$dir/lock" 2>/dev/null || date +%s) > 5 )); then
      # Owner killed between mkdir and writing its pid (story 001-7949). rmdir fails
      # if a new owner has written its pid meanwhile, so a live lock is never taken.
      rmdir "$dir/lock" 2>/dev/null
    fi
    sleep 1
  done
  echo $$ >"$dir/lock/pid"
}
unlock() { rm -rf "$dir/lock"; }

if [[ ${1:-} == status || -z ${1:-} ]]; then
  echo "running=$(running) waiting=$(count waiting) gates=$(count gates) free=$(free_pct)% load1=$(load1) disk=$(disk_gib)GiB"
  for f in "$dir"/running/* "$dir"/gates/*; do [[ -e $f ]] && echo "  ${f##*/}: $(cat "$f")"; done
  if [[ -z ${BUILD_SLOT_DIR:-} && -f $HOME/.ssh/rb_graviton ]]; then
    "$rb_bin" status 2>/dev/null || echo 'remote unavailable'
  fi
  exit 0
fi

if [[ $1 == --gate ]]; then
  shift
  waited=0
  while :; do
    lock
    if ! frozen && ! busy && (( $(count gates) < max_gates )); then
      echo "$(date '+%H:%M:%S') ${PWD##*/}: gate: $*" >"$dir/gates/$$"
      unlock
      break
    fi
    unlock
    (( waited % 300 == 0 )) && echo "build-slot: gate waiting for normal builds and gates" >&2
    sleep "$poll"; waited=$((waited + poll))
  done
  echo "build-slot: gate admitted after ${waited}s" >&2
  # Own process group for the root; signal_tree and end_tree add the descendants outside it.
  set -m
  nice -n 19 "$@" & pid=$!
  set +m
  trap 'end_tree "$pid"; rm -f "$dir/gates/$$"; exit 143' TERM INT HUP
  trap 'rm -f "$dir/gates/$$"' EXIT
  stopped=0 sharing=0
  while kill -0 "$pid" 2>/dev/null; do
    if busy && (( sharing )); then
      :
    elif busy; then
      # Every poll, not once: a process that escaped the first walk is caught by the next.
      signal_tree STOP "$pid"
      (( stopped == 0 )) && echo "build-slot: gate stopped for a normal build" >&2
      if (( stopped >= preempt )); then
        # Best effort: members may already be exiting; macOS answers EPERM for a group
        # of zombies ("kill: (-PGID) - Operation not permitted", gate-fleet 2026-09-27).
        # end_tree ends in KILL, so this wait cannot block on a command that ignores TERM.
        end_tree "$pid"; wait "$pid" 2>/dev/null
        echo "build-slot: gate killed after ${stopped}s stopped; re-run it" >&2
        exit 75
      fi
      if (( stall < preempt && stopped >= stall )); then
        signal_tree CONT "$pid"; sharing=1
        echo "build-slot: gate resumed after ${stopped}s stopped; it runs beside the build" >&2
      fi
      stopped=$((stopped + poll))
    elif (( stopped > 0 || sharing )); then
      (( sharing )) || signal_tree CONT "$pid"
      stopped=0 sharing=0; echo "build-slot: gate resumed" >&2
    fi
    sleep "$poll"
  done
  wait "$pid"
  exit
fi

waited=0
trap 'rm -f "$dir/waiting/$$"' EXIT
while :; do
  lock
  read_conf
  n=$(running) free=$(free_pct) load=$(load1) disk=$(disk_gib)
  need=$(need_free "$n") maxload=$(need_load "$n")
  if ! frozen && (( n < max && free >= need && disk >= 25 && load <= maxload )); then
    echo "$(date '+%H:%M:%S') ${PWD##*/}: $*" >"$dir/running/$$"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(date '+%F %T')" $$ "$n" "$free" "$load" "$(agents)" "$need" "$waited" "${PWD##*/}" >>"$dir/admissions.tsv"
    rm -f "$dir/waiting/$$"
    unlock
    break
  fi
  : >"$dir/waiting/$$"
  (( disk < 25 )) && reclaim_incremental
  unlock
  (( waited % 300 == 0 )) && frozen && echo "build-slot: waiting for BUILD_FREEZE to lift ($freeze_file)" >&2
  (( waited % 300 == 0 )) && echo "build-slot: waiting (running=$n free=$free% need=$need% load1=$load disk=${disk}GiB)" >&2
  sleep "$poll"; waited=$((waited + poll))
done
trap 'rm -f "$dir/running/$$"' EXIT
echo "build-slot: admitted after ${waited}s (running=$n free=$free%)" >&2
start=$(date +%s)
nice -n 5 "$@"; rc=$?
# start, seconds, builds running at admission, exit code, dir, command: tuning data.
printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$(date -r "$start" '+%F %T')" $(( $(date +%s) - start )) "$n" "$rc" "${PWD##*/}" "${*:0:80}" >>"$dir/builds.tsv"
exit $rc
}
