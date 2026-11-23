# Git and repositories (2)

## Delete a squash-merged worktree — Rust restart required

- [ ] After the backend restart, squash-merge a worktree branch into main (GitHub squash or `git merge --squash`), fetch, then open Delete on the worktree. The commit check must read "All changes are in … (squash merge)", Delete must be enabled and remove folder and branch. A merge-commit/fast-forward branch must still read "All commits are in …"; a branch with an unmerged commit must stay blocked. Unit coverage: `archive_checklist_allows_delete_after_squash_merge`.

## Descriptive worktree branch names from agents — Rust restart required

- [ ] After the backend restart, ask an agent to create a worktree through MCP. The `repo` tool description must tell it to pass `branch` (e.g. `fix/<topic>`), and the new worktree must use that name instead of a random adjective-character name.

## Merged-PR proof from poller state — Rust restart required

- [ ] After the backend restart, with a monitored repo whose branch PR is merged, a sidebar refresh must mark the worktree merged without a `gh api graphql` call (watch `/logs` or the network); a repo whose lookup fails (go offline) must not retry on each file save for 60 s. Unit coverage: `merged_proof_hits_poller_state_without_graphql`, `failed_lookup_is_cached_briefly`.
## New comments since the last visit on issues — Rust restart required

- [ ] After the backend restart, expand an open issue in the GitHub panel once (first visit: nothing marked new). Add a comment on GitHub, wait for the issue poll, collapse and expand it again: the pill must read "💬 N · 1 new", the why line "1 new comment since the last visit", Reply must be the primary action beside a secondary Work on it, Comments must be open with the new comment marked "New". Expand it a third time: nothing new. Unit coverage: `work_item_visits::tests`, `IssueDetailContent.test.tsx`.
