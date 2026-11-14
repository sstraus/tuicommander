# Manual checks

- Remote worktrees (#150): register a repository on an SSH or Direct connection, create and remove a worktree on that host (using Git and MCP), and check that the desktop sidebar updates without switching repositories. Restart the daemon, repeat, and confirm updates resume. Keep a local repository open during the check and verify its workspace rows and terminals are unaffected.
