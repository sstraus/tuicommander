# Transient reasoning story

- Story: `3038-8fbc`, pending, P2 feature.
- Plan: `/Users/stefano.straus/Gits/personal/ego/plans/improvements-0.6.md`, Step 7.
- Delivery boundary: ego 0.6 or later. A different TUICommander release number does not permit earlier delivery without an owner scope change.
- Existing starting points: `acpTranscript.ts` accepts `agent_thought_chunk`; `Transcript.tsx` renders thought entries. The story defines transient display and lifecycle behavior, not deletion of stored evidence.
- The story CLI warned that it ignored an external plan reference. `show` displays the plan and step fields, but automatic plan linkage is not verified. The reference is also in the problem statement and worklog.
- The CLI story listing showed no uncompleted matching reasoning, thinking, or thought story before creation. No implementation, build, runtime, or test was performed.
