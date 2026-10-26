# CLI signature investigation (2129-1dfd)

## Findings

The CLI status query itself opens the installed executable for writing:
`get_cli_status` → `install_path_writable` → `OpenOptions::write(true).open`.
This runs during application bootstrap and when the General Settings page mounts.
The open follows `/usr/local/bin/tuic` into the main checkout sidecar. It does not
truncate or write bytes. This path was absent from the previous sidecar-writer audits.

Native probes on 2026-10-22 copied the current CLI into this worktree's `.tmp/`.
They executed `--version`, opened and closed the copy with `os.O_WRONLY`, then
executed `--version` again. Initial results were:

| Probe | Before | After | Bytes, inode, mtime |
| --- | --- | --- | --- |
| Read-only open | exit 0 | exit 0 | unchanged |
| Write-only open, no writes | exit 0 | SIGKILL (-9) | unchanged |
| Write-only open and fsync, no writes | exit 0 | SIGKILL (-9) | unchanged |

The successful version output was `tuic 2.0.0-beta.1`. Four later byte-copy probes
of the tracked `tuic 1.7.7` sidecar also returned 0 before the write-open and -9
after it. Other probes returned 0 both times. The effect is **intermittent**;
copy mechanism, signature-cache state and timing have not been isolated.
No claim about APFS clone behavior follows from these observations.

The historical report
`tuic-aarch64-apple-darwin-2026-10-22-112657.ips` records
`SIGKILL (Code Signature Invalid)` and `Taskgated Invalid Signature`.
It does not identify who previously opened the file. Its parent is an exited
process, responsible application iTerm2. The status-query path is a reproduced
signature-sensitive operation, not a syscall trace of that historical event.
No `fs_usage` trace of the live application was taken.

## Candidate audit

These are source observations, not historical process attribution:

- `build-sidecar.mjs` stages at a UUID sibling, validates it, then calls
  `renameSync`. Make's sidecar build invokes this script.
- `agent-test.py` refuses the main checkout and copies Cargo artifacts to its
  worktree-local `.tmp/agent-test` directory through staging and replacement.
- `mutants.sh` copies into a disposable clone through staging and `mv`.
- `build_sidecars.rs` disables Tauri's reverse sidecar copy into Cargo targets.
- `tuic_cli.rs` install and auto-update already stage and rename the install
  directory entry. They do not follow its final symlink during replacement.
- Worktree warming skips existing destinations. It copies out of the parent;
  it does not publish into the parent sidecar path.
- No mbx artifact-restore or Git write was recorded at either incident time.
  Neither is attributed as the historical writer.

## Fix

The status probe must not open the installed executable for writing. Atomic
updates need permission to create a sibling and replace a directory entry,
not permission to edit the old file. The probe now creates and drops a uniquely
named empty temporary sibling. It leaves the executable and any symlink target
untouched. Windows additionally rejects a read-only destination, which Windows
cannot replace. This is an advisory capability check; permissions can change
before an update, whose actual errors remain authoritative.

The worst case is one empty temporary sibling per status call. Concurrent calls
use distinct names. Dropping the temporary file removes it. No API or transport
shape changes.

## Test evidence and limits

Pre-fix commit: `46c1e030337b76673090b9544d2b60773b04278c`.

- Deterministic RED:
  `cli_update_probe_uses_install_directory_not_executable_permissions` failed on
  rb: the old function returned false for a mode-0555 executable in a writable
  directory. Exit 100, result `20261022T100401Z-51485`.
  Log: `~/Gits/.tmp/results/tuic-2129-red-directory.log`.
- Native execution coverage:
  `cli_status_preserves_signed_cli_execution` executes a copied recorded Mach-O
  through a symlink before and after the real status query, and checks unchanged
  bytes and mtime. Both corrected pre-fix variants passed. This is not claimed
  as RED for the intermittent signature kill.
- The first native run failed before the probe because the tracked fixture is
  mode 0644. The test now sets execute permission only on its scratch copies.
- Linux cannot run the macOS-only execution case. The initial rb invocation
  selected zero tests and exited 4; that was not a passing test.
- No post-fix GREEN run was made: the managed story policy reserves it for the
  coordinator. Run `tuic_cli::tests` on rb and the signed execution case natively
  on macOS. No desktop application or installed CLI was replaced or restarted.

## Why the tracked binary is modified

The repository records the sidecar as a mode-0644 `tuic 1.7.7` Mach-O. The current
main checkout file is an executable `tuic 2.0.0-beta.1` Mach-O. Build-sidecar
publication intentionally places build output at that tracked path, while the
installed symlink points there. This explains the content/mode difference, not
who built that particular artifact.

The live target was not reset, removed, untracked, committed or replaced. Doing
so would change Boss's installed CLI, which this assignment forbids. Moving the
installation away from a tracked build artifact needs a separately authorized
migration. The original historical-writer, deterministic SIGKILL RED and live
tracked-file-resolution criteria therefore remain open.
