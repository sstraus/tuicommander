# Recorded ownership inventory (2583-fd08)

Captured read-only on 2026-10-31 from the running default instance's
`repositories.json` (resolved through `config::load_repositories`).
Source document SHA-256: `14163e59801e28da93c9f78ec7491893003ed0f754ab94a4bff12a1803a93282`.

The fixture retains both ego repository rows and all their workspace ownership
fields. Terminal data and unrelated fields are omitted. The private path prefix
is replaced with `$ROOT`. Git `.git` pointers and `commondir` files are recorded
from these checkouts; the test materializes their recorded bytes under its root.

The main row lists history-anchors; the separately registered history-anchors
row lists master and siblings. All parentRepoPath values are null. This is a
valid shared Git inventory, not nested ownership or a symlink cycle. The only
non-self ownership cycle in the complete live document was this ego pair.

`createRepositoryRefreshCoordinator` persists the shared worktree inventory for
each registered checkout. `4722b3e4fb` already did this before Progress ownership
was added in `322b290f9f`. The original resolver inferred every inventory row's
owner from its containing registration; `8d414d1d91` removed self-edges only.
This duplicated-registration shape never worked, rather than regressing in a
recent config writer. No live config was changed.
