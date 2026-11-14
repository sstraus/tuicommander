# GitHub issue schema capture

Recorded on 2026-10-28 with `gh api graphql`, querying the public issue
https://github.com/sstraus/tuicommander/issues/119. The response is unmodified.
The query selects the fields consumed by `parse_issue_node`, including
`authorAssociation`. Tests explicitly change the state and labels to exercise
polling scenarios. No GitHub write was performed.
