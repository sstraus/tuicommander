//! Commit graph lane assignment for visual git log rendering.
//!
//! Parses `git log --all --topo-order` output and assigns each commit
//! a column (lane) and color index, producing connection metadata for
//! Bezier curve rendering in the frontend.

use std::path::Path;

use serde::Serialize;

use crate::git_cli::git_cmd;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug, Clone)]
pub struct GraphNode {
    pub hash: String,
    pub column: usize,
    pub row: usize,
    pub color_index: usize,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub connections: Vec<Connection>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Connection {
    pub from_col: usize,
    pub from_row: usize,
    pub to_col: usize,
    pub to_row: usize,
    pub color_index: usize,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// A raw commit parsed from git log output.
#[derive(Debug, Clone)]
struct RawCommit {
    hash: String,
    parents: Vec<String>,
    refs: Vec<String>,
}

/// Parse git log output where each line is `hash\0parents\0refs`.
fn parse_git_log(output: &str) -> Vec<RawCommit> {
    output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '\0').collect();
            if parts.is_empty() {
                return None;
            }
            let hash = parts[0].to_string();
            let parents: Vec<String> = if parts.len() > 1 && !parts[1].is_empty() {
                parts[1].split(' ').map(|s| s.to_string()).collect()
            } else {
                Vec::new()
            };
            let refs: Vec<String> = if parts.len() > 2 && !parts[2].is_empty() {
                parts[2]
                    .split(", ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                Vec::new()
            };
            Some(RawCommit {
                hash,
                parents,
                refs,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Lane assignment algorithm
// ---------------------------------------------------------------------------

/// Assign lanes (columns) to a list of commits in topo order.
///
/// Each slot in `active_lanes` holds the hash that lane is "expecting"
/// (i.e., a parent hash that hasn't appeared as a commit yet).
fn assign_lanes(commits: &[RawCommit]) -> Vec<GraphNode> {
    // Build a lookup: hash → row index (for connection targets)
    let hash_to_row: std::collections::HashMap<&str, usize> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.as_str(), i))
        .collect();

    let mut active_lanes: Vec<Option<String>> = Vec::new();
    // Track which color index was assigned to each lane when it was created
    let mut lane_colors: Vec<usize> = Vec::new();
    let mut next_color: usize = 0;
    let mut nodes: Vec<GraphNode> = Vec::with_capacity(commits.len());

    for (row, commit) in commits.iter().enumerate() {
        // (a) Find if any active lane is expecting this commit's hash
        let existing_lane = active_lanes
            .iter()
            .position(|slot| slot.as_deref() == Some(&commit.hash));

        let column = if let Some(col) = existing_lane {
            col
        } else {
            // (b) New branch head — assign first free lane or push new
            let free = active_lanes.iter().position(|slot| slot.is_none());
            if let Some(col) = free {
                lane_colors[col] = next_color;
                next_color += 1;
                col
            } else {
                active_lanes.push(None);
                lane_colors.push(next_color);
                next_color += 1;
                active_lanes.len() - 1
            }
        };

        let color_index = lane_colors[column] % 8;

        // (d) Handle parents
        // Close any OTHER lanes that were also expecting this commit
        // (happens with merge commits where multiple lanes converge)
        for i in 0..active_lanes.len() {
            if i != column && active_lanes[i].as_deref() == Some(&commit.hash) {
                active_lanes[i] = None;
            }
        }

        let mut connections = Vec::new();

        if commit.parents.is_empty() {
            // (e) Root commit — free the lane
            active_lanes[column] = None;
        } else {
            // First parent inherits this commit's lane
            active_lanes[column] = Some(commit.parents[0].clone());

            // Build connection to first parent
            if let Some(&parent_row) = hash_to_row.get(commit.parents[0].as_str()) {
                connections.push(Connection {
                    from_col: column,
                    from_row: row,
                    to_col: column, // will be corrected when parent is processed
                    to_row: parent_row,
                    color_index,
                });
            }

            // Other parents: find a free lane or create one
            for parent_hash in commit.parents.iter().skip(1) {
                // Check if any lane already expects this parent (another branch)
                let parent_lane =
                    active_lanes
                        .iter()
                        .position(|slot| slot.as_deref() == Some(parent_hash.as_str()));

                let parent_col = if let Some(col) = parent_lane {
                    // Lane already expects this parent, just record connection
                    col
                } else {
                    // Find free lane or create new
                    let free = active_lanes.iter().position(|slot| slot.is_none());
                    if let Some(col) = free {
                        lane_colors[col] = next_color;
                        next_color += 1;
                        active_lanes[col] = Some(parent_hash.clone());
                        col
                    } else {
                        active_lanes.push(Some(parent_hash.clone()));
                        lane_colors.push(next_color);
                        next_color += 1;
                        active_lanes.len() - 1
                    }
                };

                let parent_color = lane_colors[parent_col] % 8;
                if let Some(&parent_row) = hash_to_row.get(parent_hash.as_str()) {
                    connections.push(Connection {
                        from_col: column,
                        from_row: row,
                        to_col: parent_col,
                        to_row: parent_row,
                        color_index: parent_color,
                    });
                }
            }
        }

        nodes.push(GraphNode {
            hash: commit.hash.clone(),
            column,
            row,
            color_index,
            parents: commit.parents.clone(),
            refs: commit.refs.clone(),
            connections,
        });
    }

    nodes
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn get_commit_graph(path: String, count: Option<u32>) -> Result<Vec<GraphNode>, String> {
    let count = count.unwrap_or(200).min(1000);
    let repo_path = Path::new(&path);

    let output = git_cmd(repo_path)
        .args([
            "log",
            "--branches",
            "--tags",
            "--remotes",
            "--topo-order",
            &format!("-n{count}"),
            "--pretty=format:%H%x00%P%x00%D",
        ])
        .run()
        .map_err(|e| e.to_string())?;

    let commits = parse_git_log(&output.stdout);
    Ok(assign_lanes(&commits))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Unit tests with synthetic data (no git repo needed)
    // -----------------------------------------------------------------------

    /// Helper: build a RawCommit from shorthand.
    fn raw(hash: &str, parents: &[&str], refs: &[&str]) -> RawCommit {
        RawCommit {
            hash: hash.to_string(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
            refs: refs.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_linear_history_single_column() {
        // A→B→C (A is newest)
        let commits = vec![
            raw("A", &["B"], &[]),
            raw("B", &["C"], &[]),
            raw("C", &[], &[]),
        ];
        let nodes = assign_lanes(&commits);

        assert_eq!(nodes.len(), 3);
        // All should be in column 0
        assert_eq!(nodes[0].column, 0, "A should be column 0");
        assert_eq!(nodes[1].column, 0, "B should be column 0");
        assert_eq!(nodes[2].column, 0, "C should be column 0");

        // Row assignments match order
        assert_eq!(nodes[0].row, 0);
        assert_eq!(nodes[1].row, 1);
        assert_eq!(nodes[2].row, 2);

        // All same color (same lane)
        assert_eq!(nodes[0].color_index, nodes[1].color_index);
        assert_eq!(nodes[1].color_index, nodes[2].color_index);
    }

    #[test]
    fn test_branch_and_merge() {
        // Topo order: A (merge of B,C) → B → C → D (common ancestor)
        //   A merges B and C
        //   B's parent is D
        //   C's parent is D
        let commits = vec![
            raw("A", &["B", "C"], &["HEAD"]),
            raw("B", &["D"], &[]),
            raw("C", &["D"], &[]),
            raw("D", &[], &[]),
        ];
        let nodes = assign_lanes(&commits);

        assert_eq!(nodes.len(), 4);

        // A is in column 0 (first commit, gets a lane)
        assert_eq!(nodes[0].column, 0, "A (merge) at column 0");

        // B inherits A's lane (first parent), so column 0
        assert_eq!(nodes[1].column, 0, "B (first parent of A) at column 0");

        // C should be in a different column (second parent of A)
        assert_ne!(
            nodes[2].column, nodes[1].column,
            "C should be in a different column than B"
        );

        // A should have 2 connections (to B and to C)
        assert_eq!(nodes[0].connections.len(), 2, "A has 2 parent connections");

        // D is the common ancestor — at least one lane should reach it
        // D should be in column 0 (inherits B's lane as B's first parent)
        assert_eq!(nodes[3].column, 0, "D at column 0 (inherits from B)");
    }

    #[test]
    fn test_octopus_merge() {
        // O merges P1, P2, P3; each parent is a root
        let commits = vec![
            raw("O", &["P1", "P2", "P3"], &[]),
            raw("P1", &[], &[]),
            raw("P2", &[], &[]),
            raw("P3", &[], &[]),
        ];
        let nodes = assign_lanes(&commits);

        assert_eq!(nodes.len(), 4);

        // O should have 3 connections
        assert_eq!(
            nodes[0].connections.len(),
            3,
            "Octopus merge has 3 connections"
        );

        // P1 inherits O's lane
        assert_eq!(nodes[1].column, nodes[0].column);

        // P2 and P3 should be in different columns from P1
        assert_ne!(nodes[2].column, nodes[1].column);
        assert_ne!(nodes[3].column, nodes[1].column);
    }

    #[test]
    fn test_multiple_branch_heads() {
        // Two unrelated branches: A→B and C→D (no shared ancestors)
        // Topo order could interleave them
        let commits = vec![
            raw("A", &["B"], &["main"]),
            raw("C", &["D"], &["feature"]),
            raw("B", &[], &[]),
            raw("D", &[], &[]),
        ];
        let nodes = assign_lanes(&commits);

        assert_eq!(nodes.len(), 4);

        // A gets column 0
        assert_eq!(nodes[0].column, 0);

        // C is a new branch head not expected by any lane → different column
        assert_ne!(
            nodes[1].column, nodes[0].column,
            "Unrelated branch heads should be in different columns"
        );

        // B is expected by A's lane → column 0
        assert_eq!(nodes[2].column, 0, "B inherits A's lane");

        // D is expected by C's lane
        assert_eq!(
            nodes[3].column, nodes[1].column,
            "D inherits C's lane"
        );
    }

    #[test]
    fn test_root_commit_frees_lane() {
        // After a root commit, its lane should be freed for reuse
        // A→B (root), then C→D (unrelated)
        let commits = vec![
            raw("A", &["B"], &[]),
            raw("B", &[], &[]),
            raw("C", &["D"], &[]),
            raw("D", &[], &[]),
        ];
        let nodes = assign_lanes(&commits);

        // B is a root → frees column 0
        // C should reuse column 0 (or at least get a free lane)
        // The key assertion: we don't endlessly grow columns
        let max_col = nodes.iter().map(|n| n.column).max().unwrap_or(0);
        assert!(
            max_col <= 1,
            "Lanes should be reused after root commits, max_col={max_col}"
        );
    }

    #[test]
    fn test_parse_git_log_format() {
        let output = "abc123\0def456 ghi789\0HEAD -> main, origin/main\naaa111\0\0tag: v1.0\nbbb222\0ccc333\0\n";
        let commits = parse_git_log(output);

        assert_eq!(commits.len(), 3);

        assert_eq!(commits[0].hash, "abc123");
        assert_eq!(commits[0].parents, vec!["def456", "ghi789"]);
        assert_eq!(
            commits[0].refs,
            vec!["HEAD -> main", "origin/main"]
        );

        assert_eq!(commits[1].hash, "aaa111");
        assert!(commits[1].parents.is_empty());
        assert_eq!(commits[1].refs, vec!["tag: v1.0"]);

        assert_eq!(commits[2].hash, "bbb222");
        assert_eq!(commits[2].parents, vec!["ccc333"]);
        assert!(commits[2].refs.is_empty());
    }

    #[test]
    fn test_empty_input() {
        let commits = parse_git_log("");
        assert!(commits.is_empty());
        let nodes = assign_lanes(&commits);
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_single_commit() {
        let commits = vec![raw("A", &[], &["HEAD"])];
        let nodes = assign_lanes(&commits);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].column, 0);
        assert_eq!(nodes[0].row, 0);
        assert!(nodes[0].connections.is_empty());
        assert_eq!(nodes[0].refs, vec!["HEAD"]);
    }

    #[test]
    fn test_connections_point_to_correct_rows() {
        // A→B→C linear
        let commits = vec![
            raw("A", &["B"], &[]),
            raw("B", &["C"], &[]),
            raw("C", &[], &[]),
        ];
        let nodes = assign_lanes(&commits);

        // A's connection should point to B (row 1)
        assert_eq!(nodes[0].connections.len(), 1);
        assert_eq!(nodes[0].connections[0].from_row, 0);
        assert_eq!(nodes[0].connections[0].to_row, 1);

        // B's connection should point to C (row 2)
        assert_eq!(nodes[1].connections.len(), 1);
        assert_eq!(nodes[1].connections[0].from_row, 1);
        assert_eq!(nodes[1].connections[0].to_row, 2);

        // C (root) has no connections
        assert!(nodes[2].connections.is_empty());
    }

    #[test]
    fn test_color_index_within_bounds() {
        // Create enough branches to exceed 8 colors
        let mut commits = Vec::new();
        for i in 0..12 {
            commits.push(RawCommit {
                hash: format!("H{i}"),
                parents: vec![format!("P{i}")],
                refs: vec![format!("branch{i}")],
            });
        }
        for i in 0..12 {
            commits.push(RawCommit {
                hash: format!("P{i}"),
                parents: vec![],
                refs: vec![],
            });
        }

        let nodes = assign_lanes(&commits);
        for node in &nodes {
            assert!(
                node.color_index < 8,
                "color_index {} should be < 8 for hash {}",
                node.color_index,
                node.hash
            );
        }
    }

    // -----------------------------------------------------------------------
    // Integration test with a real temp git repo
    // -----------------------------------------------------------------------

    #[test]
    fn test_end_to_end_with_real_repo() {
        use std::process::Command;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();

        // Helper to run git commands in the temp repo
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .current_dir(&path)
                .args(args)
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@test.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@test.com")
                .output()
                .unwrap_or_else(|e| panic!("git {args:?} failed: {e}"));
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).to_string()
        };

        // Build a small repo: main with 2 commits, a branch with 1 commit
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        // Disable hooks to avoid interference from global git templates
        git(&["config", "core.hooksPath", "/dev/null"]);

        std::fs::write(path.join("a.txt"), "a").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-m", "feat: first", "--no-verify"]);

        std::fs::write(path.join("b.txt"), "b").unwrap();
        git(&["add", "b.txt"]);
        git(&["commit", "-m", "feat: second", "--no-verify"]);

        // Create a branch
        git(&["checkout", "-b", "feature"]);
        std::fs::write(path.join("c.txt"), "c").unwrap();
        git(&["add", "c.txt"]);
        git(&["commit", "-m", "feat: feature commit", "--no-verify"]);

        // Back to main, add another commit
        git(&["checkout", "main"]);
        std::fs::write(path.join("d.txt"), "d").unwrap();
        git(&["add", "d.txt"]);
        git(&["commit", "-m", "feat: third on main", "--no-verify"]);

        // Call the actual command
        let result = get_commit_graph(path.to_string_lossy().to_string(), Some(50));
        assert!(result.is_ok(), "get_commit_graph failed: {result:?}");

        let nodes = result.unwrap();
        // We have exactly 4 commits
        assert_eq!(nodes.len(), 4, "Expected 4 commits, got {}", nodes.len());

        // All nodes should have valid columns and rows
        for (i, node) in nodes.iter().enumerate() {
            assert_eq!(node.row, i, "Row should match index");
            assert!(node.color_index < 8, "Color index in bounds");
        }

        // There should be exactly one root commit (no parents)
        let root = nodes.iter().find(|n| n.parents.is_empty());
        assert!(root.is_some(), "Should have a root commit");

        // At least one node should have refs
        let has_refs = nodes.iter().any(|n| !n.refs.is_empty());
        assert!(has_refs, "At least one commit should have refs");

        // Should use at least 2 columns (main + feature branch)
        let max_col = nodes.iter().map(|n| n.column).max().unwrap_or(0);
        assert!(max_col >= 1, "Should have at least 2 columns for 2 branches");
    }

    #[test]
    fn test_merge_connections_are_correct() {
        // A merges B and C, B→D, C→D
        let commits = vec![
            raw("A", &["B", "C"], &[]),
            raw("B", &["D"], &[]),
            raw("C", &["D"], &[]),
            raw("D", &[], &[]),
        ];
        let nodes = assign_lanes(&commits);

        // A has connections to B (row 1) and C (row 2)
        let a_conns = &nodes[0].connections;
        assert_eq!(a_conns.len(), 2);

        let to_rows: Vec<usize> = a_conns.iter().map(|c| c.to_row).collect();
        assert!(to_rows.contains(&1), "A should connect to B (row 1)");
        assert!(to_rows.contains(&2), "A should connect to C (row 2)");

        // C should connect to D (row 3)
        let c_conns = &nodes[2].connections;
        assert_eq!(c_conns.len(), 1);
        assert_eq!(c_conns[0].to_row, 3, "C should connect to D (row 3)");
    }
}
