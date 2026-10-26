use super::*;
use crate::stories::PlanPatch;

fn setup() -> (tempfile::TempDir, StoryStore, Plan) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
    let plan = store
        .create_plan(NewPlan {
            project: "/project".into(),
            title: "Original".into(),
            source: "original.md".into(),
        })
        .unwrap();
    (dir, store, plan)
}

/// Catches: stale independent writers overwriting a plan or mixing two atomic patches.
#[test]
fn update_plan_competing_connections_accept_exactly_one_patch() {
    let (dir, first, plan) = setup();
    let second = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [first, second]
        .into_iter()
        .enumerate()
        .map(|(i, store)| {
            let barrier = barrier.clone();
            let plan = plan.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.update_plan(
                    &plan.id,
                    plan.revision,
                    PlanPatch {
                        title: Some(format!("Writer {i}")),
                        source: Some(format!("writer-{i}.md")),
                    },
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .find_map(|r| r.as_ref().err())
            .unwrap()
            .contains("stale plan revision")
    );
    let winner = results.into_iter().find_map(Result::ok).unwrap();
    assert_eq!(winner.revision, 2);
    assert!(matches!(
        (winner.title.as_str(), winner.source.as_str()),
        ("Writer 0", "writer-0.md") | ("Writer 1", "writer-1.md")
    ));
    let reopened = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
    assert_eq!(reopened.get_plan(&plan.id).unwrap(), winner);
    assert_eq!(reopened.list_plans(&plan.project).unwrap(), vec![winner]);
}

/// Catches: partial updates clearing omitted metadata, or invalid patches committing half a change.
#[test]
fn update_plan_preserves_omitted_fields_and_rejects_invalid_patches_atomically() {
    let (_dir, store, plan) = setup();
    for patch in [
        PlanPatch::default(),
        PlanPatch {
            title: Some("Corrected".into()),
            source: Some(" ".into()),
        },
        PlanPatch {
            title: Some("x".repeat(201)),
            source: None,
        },
        PlanPatch {
            title: None,
            source: Some("x".repeat(4097)),
        },
    ] {
        assert!(store.update_plan(&plan.id, plan.revision, patch).is_err());
        assert_eq!(store.get_plan(&plan.id).unwrap(), plan);
    }
    let renamed = store
        .update_plan(
            &plan.id,
            plan.revision,
            PlanPatch {
                title: Some(" Corrected ".into()),
                source: None,
            },
        )
        .unwrap();
    assert_eq!(renamed.title, "Corrected");
    assert_eq!(renamed.source, plan.source);
    let moved = store
        .update_plan(
            &plan.id,
            renamed.revision,
            PlanPatch {
                title: None,
                source: Some(" moved.md ".into()),
            },
        )
        .unwrap();
    assert_eq!(moved.title, renamed.title);
    assert_eq!(moved.source, "moved.md");
    assert_eq!(moved.revision, 3);
}

/// Catches: opening an existing v3 store losing plan data instead of supplying revision one.
#[test]
fn update_plan_migrates_version_three_without_changing_metadata() {
    let (dir, store, plan) = setup();
    drop(store);
    let path = dir.path().join("stories.sqlite3");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("ALTER TABLE plans DROP COLUMN revision; ALTER TABLE plans DROP COLUMN archived; PRAGMA user_version=3;")
        .unwrap();
    drop(conn);
    let store = StoryStore::open_at(&path).unwrap();
    assert_eq!(store.get_plan(&plan.id).unwrap(), plan);
    let changed = store
        .update_plan(
            &plan.id,
            1,
            PlanPatch {
                title: Some("Migrated".into()),
                source: None,
            },
        )
        .unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(changed.source, plan.source);
}
