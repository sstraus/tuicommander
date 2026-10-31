# Managed child progress reporting repair

Native story: `01a19313-1d6c-70f8-954c-d03b4874d006`.
Wiz trace: `2597-76c6`.

## Contract

Managed children report final outcomes to their parent through agent mail.
A child progress call is refused before journal writes, notifications, or waiting-state changes.
The error identifies the actual parent and the agent send command.
Root progress keeps its current behavior.

## Approach

Use the MCP caller-to-peer binding and the shared session parent map.
Use the same parent resolver for handler enforcement and MCP instructions.
Remove the root progress duty from child launch guidance.
Keep refusal explicit rather than forwarding progress through a second mail API.

## Evidence

The original progress handler in commit `2268b42d4b` has no parent check.
The native launch playbook introduced in `c7d065de61` also appends the root reporting duty to children.
The corrected handler regression on base `4dffa5081ff0780c371528b87f430fea8361a5bb` receives a persisted entry receipt instead of a refusal.
Three instruction regressions fail on the same base.
Warnings-denied Clippy passes in the RED runs.

## Validation

Run only progress, MCP instruction, and launch instruction regressions on rb.
The parent owns independent review, integration, and any wider batch gates.
A running desktop backend requires a manual restart to load Rust changes.
