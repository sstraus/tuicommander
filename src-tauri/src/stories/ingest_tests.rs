use super::*;
use std::sync::Arc;

fn fixture() -> (tempfile::TempDir, Arc<StoryStore>, Story) {
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let store = Arc::new(StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap());
    let plan = store
        .create_plan(NewPlan {
            project: dir.path().to_string_lossy().into_owned(),
            title: "Ingest plan".into(),
            source: "plan.md".into(),
        })
        .unwrap();
    let story = store
        .create_story(NewStory {
            plan_id: plan.id,
            title: "Searchable native story".into(),
            criteria: vec!["quartz heliotrope acceptance phrase".into()],
            priority: 2,
            story_type: Some(StoryType::Feature),
            origin: StoryOrigin::Native,
            file_scope: vec![],
        })
        .unwrap();
    (dir, store, story)
}

// Catches: notes reusing the subject revision, making mdkb discard new worklog content.
#[test]
fn ingest_notes_advance_owner_revision_without_invalidating_story_proof() {
    let (_dir, store, story) = fixture();
    let before = store.ingest_document(&story.id).unwrap();
    store
        .add_note(
            &story.id,
            story.revision,
            "Observed worklog evidence",
            StoryTransitionActor::Human,
            vec![],
        )
        .unwrap();
    let after = store.ingest_document(&story.id).unwrap();
    assert_eq!(store.get_story(&story.id).unwrap().revision, story.revision);
    assert!(after.revision > before.revision);
    let doc = after.document.unwrap();
    assert!(doc.body.contains("Observed worklog evidence"));
    assert!(
        doc.body
            .contains("- [ ] quartz heliotrope acceptance phrase")
    );
    assert_eq!(doc.metadata["status"], "ready");
    assert_eq!(doc.metadata["type"], "feature");
    assert!(doc.metadata["updated_at"].as_str().unwrap().ends_with('Z'));
}

// Catches: observing rolled-back content, or dropping retry state when a store handle closes.
#[test]
fn ingest_ledger_is_atomic_and_survives_reopen_and_deletion() {
    let (dir, store, story) = fixture();
    let before = store.ingest_document(&story.id).unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("stories.sqlite3")).unwrap();
    conn.execute_batch("BEGIN IMMEDIATE; DELETE FROM stories; ROLLBACK;")
        .unwrap();
    assert_eq!(
        store.ingest_document(&story.id).unwrap().revision,
        before.revision
    );
    assert!(store.ingest_document(&story.id).unwrap().document.is_some());
    conn.execute("DELETE FROM stories WHERE id=?1", [&story.id])
        .unwrap();
    drop(store);
    let reopened = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
    let deleted = reopened.ingest_document(&story.id).unwrap();
    assert!(deleted.revision > before.revision);
    assert!(deleted.document.is_none());
    assert_eq!(reopened.ingest_ids().unwrap()[0].1[0].id, story.id);
    assert!(!dir.path().join(".tuic").exists());
}

// Catches: revision-independent notes being lost when two handles append concurrently.
#[test]
fn ingest_concurrent_notes_have_distinct_durable_owner_revisions() {
    let (dir, store, story) = fixture();
    let other = StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap();
    let before = store.ingest_document(&story.id).unwrap().revision;
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            store.add_note(
                &story.id,
                story.revision,
                "First writer",
                StoryTransitionActor::Human,
                vec![],
            )
        });
        let second = scope.spawn(|| {
            other.add_note(
                &story.id,
                story.revision,
                "Second writer",
                StoryTransitionActor::Human,
                vec![],
            )
        });
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
    });
    let after = store.ingest_document(&story.id).unwrap();
    assert_eq!(after.revision, before + 2);
    let body = after.document.unwrap().body;
    assert!(body.contains("First writer") && body.contains("Second writer"));
}

// Catches: mutation commits never reaching mdkb search, stale revisions on note/status updates,
// and reconnect replay missing changes made while the daemon was unavailable.
#[cfg(unix)]
#[tokio::test]
#[ignore = "requires an installed mdkb with external ingest support; starts an isolated daemon"]
async fn ingest_closed_story_is_findable_in_real_mdkb_without_a_story_file() {
    use crate::mdkb_client::MdkbClient;
    use std::process::Command;
    use std::time::Duration;

    let mdkb = std::env::var_os("TUIC_TEST_MDKB").unwrap_or_else(|| "mdkb".into());
    let home = tempfile::tempdir_in(crate::test_support::short_socket_test_temp_root()).unwrap();
    let (dir, store, mut story) = fixture();
    let project = dir.path().to_str().unwrap();
    std::fs::create_dir_all(home.path().join(".mdkb")).unwrap();
    std::fs::write(
        home.path().join(".mdkb/daemon.toml"),
        format!(
            "whitelist_dirs = [{}]\n",
            serde_json::to_string(project).unwrap()
        ),
    )
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(&mdkb)
            .args(args)
            .env("HOME", home.path())
            .current_dir(dir.path())
            .output()
            .unwrap()
    };
    let init = run(&["init"]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    struct StopDaemon<'a> {
        binary: &'a std::ffi::OsStr,
        home: &'a std::path::Path,
    }
    impl Drop for StopDaemon<'_> {
        fn drop(&mut self) {
            let _ = Command::new(self.binary)
                .args(["daemon", "stop"])
                .env("HOME", self.home)
                .output();
        }
    }
    let _stop = StopDaemon {
        binary: &mdkb,
        home: home.path(),
    };
    let start = run(&["serve", "--daemon", "--global", "--detach"]);
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let socket = home.path().join(".mdkb/daemon-hook.sock");
    let mut client = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(client) = MdkbClient::connect_to(Some(socket.clone())).await {
                break client;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("isolated mdkb starts");
    super::ingest::sync_store(store.clone(), &mut client)
        .await
        .unwrap();
    let stopped = run(&["daemon", "stop"]);
    assert!(stopped.status.success());
    assert!(MdkbClient::connect_to(Some(socket.clone())).await.is_err());
    story = store
        .update_story(
            &story.id,
            story.revision,
            StoryPatch {
                title: Some("Revised searchable native story".into()),
                ..Default::default()
            },
        )
        .unwrap();
    store
        .add_note(
            &story.id,
            story.revision,
            "cobalt cinnabar worklog phrase",
            StoryTransitionActor::Human,
            vec![],
        )
        .unwrap();
    for command in [
        StoryCommand::StartManual,
        StoryCommand::CheckCriterion(0),
        StoryCommand::SubmitReview,
        StoryCommand::Approve,
    ] {
        story = store
            .transition(&story.id, story.revision, command)
            .unwrap();
    }
    // Reconnect after mutations committed while the daemon was absent.
    drop(client);
    let restarted = run(&["serve", "--daemon", "--global", "--detach"]);
    assert!(restarted.status.success());
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if MdkbClient::connect_to(Some(socket.clone())).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("isolated mdkb restarts");
    let mut client = MdkbClient::connect_to(Some(socket)).await.unwrap();
    super::ingest::sync_store(store.clone(), &mut client)
        .await
        .unwrap();
    let search = run(&["search", "quartz heliotrope acceptance", "--root", project]);
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    assert!(
        String::from_utf8_lossy(&search.stdout).contains("Revised searchable native story"),
        "{}",
        String::from_utf8_lossy(&search.stdout)
    );
    let note_search = run(&["search", "cobalt cinnabar worklog", "--root", project]);
    assert!(note_search.status.success());
    assert!(
        String::from_utf8_lossy(&note_search.stdout).contains("Revised searchable native story")
    );
    let listed = client.external_list(project, "tuic-stories").await.unwrap();
    let remote = listed.iter().find(|record| record.id == story.id).unwrap();
    assert_eq!(
        remote.revision,
        store.ingest_document(&story.id).unwrap().revision
    );
    let get = run(&["get", &remote.document_id.unwrap().to_string()]);
    let text = String::from_utf8_lossy(&get.stdout);
    assert!(
        get.status.success()
            && text.contains("done")
            && text.contains("[x] quartz heliotrope acceptance phrase"),
        "{text}"
    );
    // Replaying equal revisions is a no-op, not another document.
    super::ingest::sync_store(store.clone(), &mut client)
        .await
        .unwrap();
    assert_eq!(
        client
            .external_list(project, "tuic-stories")
            .await
            .unwrap()
            .len(),
        1
    );
    let connection = rusqlite::Connection::open(dir.path().join("stories.sqlite3")).unwrap();
    connection
        .execute("DELETE FROM story_notes WHERE story_id=?1", [&story.id])
        .unwrap();
    connection
        .execute(
            "DELETE FROM story_transitions WHERE story_id=?1",
            [&story.id],
        )
        .unwrap();
    connection
        .execute("DELETE FROM stories WHERE id=?1", [&story.id])
        .unwrap();
    super::ingest::sync_store(store.clone(), &mut client)
        .await
        .unwrap();
    let deleted = client.external_list(project, "tuic-stories").await.unwrap();
    assert!(deleted[0].deleted && deleted[0].document_id.is_none());
    let search = run(&["search", "quartz heliotrope acceptance", "--root", project]);
    assert!(search.status.success());
    assert!(!String::from_utf8_lossy(&search.stdout).contains("Revised searchable native story"));
    assert!(!dir.path().join(".tuic").exists());
    assert!(!dir.path().join(format!("{}.md", story.id)).exists());
}
