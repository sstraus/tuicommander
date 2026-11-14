# Ego startup composer — 2948-fc2a

`ego-startup-2948.tcap` records installed ego 0.4.7, build b2672d965,
on macOS at 41 rows by 128 columns on 2026-11-10. The capture uses a
throwaway HOME and EGO_HOME under `~/Gits/.tmp/2948`, not the user's data.

Invocation: `ego --bare --offline --model ollama/qwen3 --thinking off --mode yolo --sandbox off`.
A temporary loopback discovery server reports qwen3 with completion and tool
capabilities and a served context length of 32768. No prompt is submitted and
no model generation occurs. Terminal cursor-position queries receive row 15,
column 1 replies. The capture retains output chunks and those input replies.
The probe process is stopped after four seconds.

The final native composer is empty. Its status has a ten-cell context bar
(`▱`/`▰`) and `trust=untrusted`, but no legacy ` · ctx ` or ` · trust=`
delimiters. TUIC previously classified this screen as Unknown, so its existing
stable-ready debounce could never release a queued initial brief or mail wake.

The queue-boundary regression replays this capture through the real VT buffer,
screen adapter and timer readiness reducer, then asserts initial-brief bytes
at the PTY writer with a mail notice queued behind the brief. It does not
pretend the recording acknowledges a turn that was never submitted to ego.

Capture SHA-256: `aef70418d10289517549dcd338c3e0b9c9f87705b71a22e6264830257894a2d8` (2303 bytes).
