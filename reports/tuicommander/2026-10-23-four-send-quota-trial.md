# Four-send input-priority quota trial — 2026-10-23

## Status and contract

**NOT LANDABLE under the requested all-statistic rule.** The four completed
acquisitions remain in A1–B1–A2–B2 order. B2 initially built but refused
acquisition because the screen locked; that no-sample attempt is retained under
`B2-locked-*` and in `series-state.json.failed_attempts`. After Boss unlocked the
console and the coordinator authorized continuation, only B2 was resumed. No
completed population was repeated or replaced. The B2 lock/unlock gap is a
runtime-load confound, not a hidden continuous series.

Both B idle p95s improve: **10/11 ms versus A 24/44 ms**. However, the 20-batch
calibration control has B1 rAF max **40 ms versus A1 26 / A2 36 ms**. The adverse
paired change is 14 ms, exceeding A's 10 ms run-to-run spread. The tail also
exceeds the worst A value by 4 ms. Under the stated strict rule this is sufficient
to withhold landing; the better B2 tail does not erase B1. No other quota or
policy change is proposed.

Candidate `94377aaf26e3d5f20a4a149d6761852fa8cf62d1` on
`fix/2040-quota-trial` changes rolling bypass capacity from two to four. Its
fixed array rotates before recording the latest send. The 8 ms spacing,
100 ms expiry, animation/load floors, serializer, receipt gate and synchronized
output rules are unchanged. Existing quota and retained-retry fixtures now
exhaust four credits. SPEC and backend PTY documentation state four.

The allowance rises from 20 to 40 bypasses/s in the sustained limit. Four cuts
can occupy 0/8/16/24 ms; the fifth cannot bypass before 100 ms. This is a
coordinator-authorized reversible policy experiment, not a proven defect fix.
No other value or policy was tried. No per-story unit GREEN suite was run;
the separate coordinator critic remains required.

The strict decision rule is B idle p95 below **every** A idle p95, with no
bulk/heartbeat statistic worse beyond A's run-to-run spread. Compare each B
control with its preceding A and permit an adverse change no larger than the absolute A1–A2 spread. The calibration rAF maximum fails this
check; all other reported control metrics pass. This is a conservative empirical
decision, not proof that the policy caused the isolated 40 ms interval.
Story criteria 3/4/24 are unchanged and unchecked.

## Controlled acquisition

Evidence and reproducible helpers:
`~/Gits/.tmp/2040-web/native-quota-four-abab/`.
Read `contract.json`, `series-state.json`, `comparison.json`, `series.py`,
`run-one.py`, `analyze-complete.py` and `resume-b2.py`. The previous recorded
harness is reused.
A = `f59ec280e`; B = `94377aaf2`. Only the policy file differs in runtime
production source; `pty.rs` and the frontend hashes match across variants.
Common temporary observers record reader completion, native cuts, IPC,
CanvasTerminal receives/acceptance and actual `fillText`. Quota observers use
the oldest real credit and the input generation, not assumed credit state.

Every completed smoke/main receive reports **37 columns × 14 rows**,
640×420 CSS pixels, DPR 2. Each lifecycle uses the same named
`agent-test-2040` instance at port **19890**, Unix-socket API, installed ego
with default configuration and no launch arguments. Only `/help` is submitted;
draft glyphs are cleared. Native binary replacement requires sequential app
lifecycles and fresh ego processes, not one conversation surviving restarts.
Builds use `make agent-test`, its build-slot runner and mbx. The daily app and
port 9876 are untouched.

These are synthetic frontend key events to actual native WebKit canvas fill,
**not trusted OS keyboard input or physical presentation**. Each completed run
has a 3-idle/3-help smoke and a 200-idle/200-help main population. No timing run
is discarded. The initial B2 attempt failed before any timing acquisition;
the resumed B2 completed with the same source and observer hashes as B1.

## Latency and quota observations

Times are **median / p95 / max**, milliseconds. Main populations only.

| Run | Idle key→fill (N=200) | Idle reader-complete→cut (N=200) | Help-candidate key→fill (N=200) | Help-candidate reader-complete→cut |
|---|---:|---:|---:|---:|
| A1 | 10 / 24 / 37 | 0.064 / 18.228 / 22.096 | 20 / 29 / 40 | 9.077 / 12.097 / 24.496 (N=199) |
| B1 | 6 / 10 / 19 | 0.053 / 0.085 / 0.105 | 20.5 / 28 / 40 | 8.673 / 12.015 / 23.810 (N=200) |
| A2 | 17 / 44 / 57 | 0.142 / 34.296 / 39.374 | 21 / 28 / 34 | 9.960 / 16.733 / 24.334 (N=200) |
| B2 | 5 / 11 / 17 | 0.051 / 0.076 / 0.109 | 20 / 27 / 39 | 8.775 / 18.035 / 20.333 (N=199) |

All 1,600 main samples have reader, receive, acceptance and fill
correlations. A1 and B2 each have one missing matching shared-cut observer during
help; their cut timestamps are not invented. Verified main help/output overlap counts
are A1/B1/A2/B2 = 195/190/196/183, so the table labels the full help populations as
candidates, not 200 verified overlaps. Cross-clock stage timing retains the
previous harness's approximately ±2 ms uncertainty.

| Quota blocked, per 200 | A1 idle | B1 idle | A2 idle | B2 idle | Help, all four runs |
|---|---:|---:|---:|---:|---:|
| At reader completion | 60 | 0 | 86 | 0 | 0 |
| At actual input arm | 64 | 0 | 89 | 0 | 0 |

Thus both B runs remove the observed quota wait in their idle populations.
Machine load is not controlled: A1 and A2 alone span 24–44 ms idle p95. The two B
populations support a repeated idle improvement in this series, not an all-load or physical-presentation claim.

## Paced bulk and heartbeat controls

The unchanged generator emits 12 UTF-8 rows every 100 ms. Each lifecycle first
runs 20 batches, then 120; only the count changes. Actual reader text is
reconstructed from byte-count-checked native read records. Its bytes match the
independent generator contract including observed PTY ONLCR conversion.
Payload includes begin/end marker text; the final marker's newline is excluded
consistently. Every completed variant has exactly the same payload hash:

| Batches | Reader bytes | SHA-256 |
|---|---:|---|
| 20 | 30,271 | `10c4501353ef3ff88dce02004d4ac7581047a6a641e6e7e16471cfdd043ca5df` |
| 120 | 181,471 | `73e26194bb288bd4f0ccb5c22bfbdee7ccd95223fcde4b30f366a6c9645c0191` |

Throughput is payload bytes divided by first-begin-read to end-marker-read
elapsed time. Heartbeats cover the frontend probe interval, including the
unchanged three-second generator startup delay, as in the inherited control.

| Run | Batches | Reader B/s | rAF p95 / max, ms | 100 ms timer p95 / max, ms |
|---|---:|---:|---:|---:|
| A1 | 20 | 14,068.50 | 18 / 26 | 105 / 105 |
| B1 | 20 | 13,931.49 | 18 / 40 | 105 / 105 |
| A2 | 20 | 13,896.92 | 18 / 36 | 105 / 106 |
| B2 | 20 | 14,519.96 | 18 / 22 | 105 / 105 |
| A1 | 120 | 13,951.80 | 18 / 19 | 105 / 106 |
| B1 | 120 | 13,950.37 | 18 / 20 | 105 / 106 |
| A2 | 120 | 13,977.79 | 18 / 22 | 105 / 106 |
| B2 | 120 | 14,555.62 | 18 / 18 | 105 / 105 |

Retain the adverse calibration tail: B1 rAF max is 40 ms versus A 26/36 ms,
not a universal heartbeat non-regression claim. At 120 batches B1 throughput
is 1.43 B/s below the lower A value; the A run-to-run spread is 25.99 B/s.
That throughput loss is smaller than A's spread and does not itself fail the
paired spread rule. All main-control statistics pass that rule, but the earlier
calibration rAF tail does not. These are paced reader-throughput observations,
not maximum-capacity benchmarks. No priority input is armed during the bulk
control, so its adverse tail is not proof of policy causation; runtime load,
frontend scheduling and the startup interval remain possible contributors.

## App process CPU during idle

`ps -p <own app PID> -o time=` is sampled at approximately 100 ms plus process
launch cost. The bracket begins before the first main idle key and ends after
its last fill. Values are app CPU time, excluding WebContent and child ego.
`ps` reports hundredths of a second; this is not a precise energy measurement.

| Run | Owned app PID | CPU seconds | Observed wall seconds | Bracket padding before / after, ms |
|---|---:|---:|---:|---:|
| A1 | 82992 | 1.17 | 15.019 | 3.792 / 98.385 |
| B1 | 13009 | 1.08 | 13.434 | 20.375 / 1.750 |
| A2 | 46770 | 1.21 | 16.478 | 23.718 / 108.176 |
| B2 | 96860 | 1.02 | 13.301 | 15.594 / 1.861 |

Do not compare absolute CPU seconds without the different elapsed durations.

## Cleanup, validation and remaining work

All five sanctioned stops return 0; supervisors return 2 after shutdown.
The initial B2 post-build failure creates no ego or workload session; resumed
B2 deletes its ego and both bulk sessions before stopping. The series restores
all temporary observers and production files to committed B source; its final
Git status is empty. The instance desktop PID file is absent. No daily-app
restart, push, merge, criterion check, additional quota value, broad test suite,
clippy or mutation run was performed.

Offline analysis succeeds and retains every completed population. Its first
foreground invocation exceeded a 10-second tool budget after writing the stage
analyses; no process remained. The identical analysis then completed through
the background wrapper. This is an analysis retry, not a timing acquisition
retry. Instruction-link validation and `git diff --check` passed before the
candidate commit.

The completed B2 uses the exact B1 production and observer hashes, and all
smoke/main grid headers match 37×14. The lock interruption is retained rather
than removed from the experiment history. Final four-run analysis succeeds;
`comparison.json.decision` records each paired adverse delta, A spread and pass
result. No further timing acquisition or policy value is authorized by this
trial. Hold the candidate for the coordinator's critic, not landing.

The changed quota and retained-retry policy tests still need the coordinator's
targeted rb GREEN and separate critic. The experimental Rust policy is not
loaded in Boss's daily app; loading any accepted Rust change requires Boss's
manual rebuild/restart. Criteria 3/4/24 remain untouched. The first final static
check passed formatting, instruction links and diff checks; story-close correctly
refused unchecked original criterion 3. Repeat only the static checks invalidated
by this final report update; no broad suite is run for a documentation amendment.
