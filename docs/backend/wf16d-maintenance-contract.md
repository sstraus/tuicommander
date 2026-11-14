# WF16D maintenance nodes — contract

Story 2527-ce1e. Status: implemented for adapter A. Adapter selected by coordinator 2026-11-05: cache/quarantine GC.

## Grounding (verified in source)

- `workflows/definition.rs` already has `NodeKind::Tool { tool_id, tool_revision, command }` next to `RunCommand`; both carry a `CommandDefinition` (`workflows/command.rs`) that runs through the WF05 executor (`command/executor.rs`) with a target (`Local`, `Rb`, `RbAfterNative`), a typed output check and a `CommandResource` reservation (`Build`, `CanonicalGit`, `Mutation`, ...).
- WF16A (`docs/backend/retained-tools.md`) keeps the operator's existing tools callable by hand. A retained tool declares id, entry point, invocation, prerequisites, owner and output receipt, and every run returns a receipt (`ran`, `entry_point_missing`, `unavailable`).
- The `workflow` MCP schema must stay below the 65536-byte `search_tools` budget (`mcp_transport_catalogue_tests.rs`: `top_schema omitted at the 65536-byte budget` fails otherwise). Every new node field costs bytes in that schema.

## Implementation (adapter A)

- `CommandResource::Maintenance(name)` in `workflows/command.rs`. No new node kind and no new field, so existing definition hashes do not change.
- `CommandDefinition::validate` refuses a node that names `Maintenance` unless the name is in `MAINTENANCE_OPERATIONS` (`cache_gc`), it has no `secret_refs` and it targets `Local`. Publication and execution both call `validate`, so a refused node returns an error and never spawns. The refusal is an `Err`, not a receipt.
- The executor journal keys a `Build` resource per effect (`build:<effect>`) and a maintenance resource per operation (`maintenance:<name>`). `admit` makes the two classes conflict: a maintenance node is held `queued` while any other effect holds a build key, and a build is held while a maintenance key is held. Two builds still run together. A queued build holds no key, so a waiting GC can be overtaken by a new build.
- The contract does not clear the environment of a running command; "no credentials" means no `secret_refs`, as for every typed command.
- Tests: `provisioning_node_requires_named_authority`, `cache_gc_keeps_live_reservation`, `builds_wait_for_a_running_gc_but_not_for_each_other` (`command/executor/tests/maintenance.rs`).

## Schema budget

Measured with `workflow_schema_keeps_a_margin_under_the_search_budget`: the variant costs about 110 bytes. After the schema room landed (7c029af455) the margin is about 7 KiB; the guard stays at 2 KiB.
