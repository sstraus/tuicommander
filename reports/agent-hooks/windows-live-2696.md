# Windows live hook check: 2026-11-11

Status: blocked. No acceptance criterion is proved by this preflight.

The authorized password-only SSH command with a separate `-l` login succeeded.
The host reported Windows 11 Pro 10.0.26200, 6,290,484 KiB visible RAM,
6,683,700 KiB total virtual memory and 12,457,750,528 bytes free on C:.
`whoami` confirmed the expected lab account. `query user` returned
`No User exists for *`: no interactive desktop session was available.

The PATH lookup found Git, but no Node, installed agent CLI or native ego.
No application executable appeared at the queried
`C:/tuic-win-8c6b6e5b/src-tauri/target/debug/*.exe` path.
This does not exclude artifacts elsewhere.

A second, bounded SSH call to inspect known artifact locations failed with
`Permission denied (publickey,password,keyboard-interactive)` before executing
its remote command. No authentication retry was made after that failure.

No build was started: free disk was below the previously recorded 15 GB stop
threshold. No application, agent, hook, console capture or desktop control was
started or changed. No answer, reject, cancel, stop or exit retraction was
observed. No screenshot or installed-agent launch pass is claimed.

Remaining inputs: reliable SSH access, a Windows application artifact or enough
build resources, installed agent CLIs and an interactive desktop login for the
controls and badge check. Earlier parser and transport results remain valid;
they were not rerun.

Raw local evidence: `~/Gits/.tmp/tuic-win-20261111/preflight.log` and
`artifacts.log`; the corresponding Python files retain the commands without
credential values. The preflight process exited 1 because optional inventory
commands had no matches; its successful OS/resource output is retained.
The artifact call exited 255 at authentication.

## Authorized setup follow-up

The coordinator subsequently authorized cache cleanup and per-user CLI installs.
Final C: free space was **16,106,627,072 bytes**, exceeding 15 GiB.

Verified executable versions: Node **v26.13.1** (official LTS archive, SHA-256
checked against its published checksum), npm **11.20.0**, Claude Code
**2.1.322**, Codex **0.167.1**, Git **2.55.0.windows.5**, rustc/cargo **1.97.0**.
No CLI login was attempted. Node resides under the user's
`AppData/Local/Programs/node-v26.13.1-win-x64`; CLI shims reside under
`AppData/Roaming/npm`. Both paths were added to the user PATH.

Deleted package-cache contents: Cargo registry source cache (1,265,894,184
bytes), empty Rust download/temp caches, npm cache (366,295,913 bytes), and
1,832 Windows Update download-cache files (3,150,299,973 bytes). The downloaded
Node archive (41,166,246 bytes) was removed after extraction. The update cleanup
checked that no update worker was active and stopped once free space reached
15 GiB. No user documents, agent state or unrelated build outputs were deleted.
The complete update-file deletion list is retained on the VM at
`C:/tuic-win-8c6b6e5b/evidence/setup-20261111/deleted-update-cache.tsv`.

npm initially rejected the certificate chain. Installation succeeded with
Node's `--use-system-ca`; TLS verification remained enabled. The user environment
now sets `NODE_USE_SYSTEM_CA=1`. npm warned about a skipped Claude postinstall
script; both CLI `--version` commands succeeded. No authenticated CLI behavior
or hook retraction is inferred from that startup check.

Evidence: `setup-inventory.log`, `setup-compressed.log`, `setup-finish.log`,
`cleanup-update.log` and `cleanup-update-rest.log` under the local evidence
folder above. Setup transport failures are also retained: an overlong encoded
command was rejected before execution, and an SSH-stdin attempt timed out
without output. Compressed command transport completed successfully.

This resolves the cleanup and CLI installation assignment. The original live
hardware criteria still require desktop/CLI login and a runnable TUIC artifact.
No build or test suite ran.
