# Automations

The backend Once runtime starts on desktop and headless hosts. It records
precheck decisions before launching the saved agent profile with the literal
prompt. Precheck output is saved in history and is not added to the prompt.
Recurring dispatch, public execution controls, completion detection and maximum
duration enforcement are not yet available. Use isolated test definitions until
the remaining execution integration is complete.

Open the command palette and select **Automations**. This dialog manages runs
on this machine. It does not dispatch onto a different saved remote connection.

Select **New automation**, enter a name, repository path, configured run config
identifier and literal prompt. Choose an existing checkout or a new worktree
per run with a base branch. The agent uses its run config's permissions.

Choose Hourly, Daily, Weekdays or Weekly, set the time controls and select
**Apply cadence**. The backend converts the cadence to cron. You can also edit a
five-field cron expression directly. For a one-time run, choose **Once** and
enter **Once local time** in the named timezone. The backend resolves the
instant; the browser does not convert it through your browser timezone.
A completed one-time occurrence stays visible without another next run. Enter an IANA timezone such as
`Europe/Madrid` and select **Preview schedule**. An empty creation zone resolves
to the backend's local zone. Preview shows backend occurrences in that named
zone. The backend validates schedules and handles daylight-saving transitions.

Set a maximum duration and catch-up grace in seconds. Overlap always skips an
occurrence while a previous run remains open. An optional precheck runs before
scheduled launches: exit 0 allows the agent to run; other exits skip quietly.
Save the definition before running it.

**Pause** stops scheduled launches; **Resume** enables them. **Run now** works
while paused, bypasses precheck, obeys capacity and overlap, and leaves the
scheduled cursor unchanged. A skip receipt is shown as a skip, not as a launch.
The list shows next-run time and last status; recent history shows saved run
statuses and reasons. **Delete** asks for confirmation and keeps saved history.

Errors stay visible and failed saves retain the draft. **Refresh** retries a
failed list load. Escape closes the dialog and Tab stays inside it.

The dialog's backend integration is pending Step 8 (story 1617). Until that API
is installed, requests show a backend error. The injectable adapter is verified
independently; this guide does not claim an installed scheduler is available.
