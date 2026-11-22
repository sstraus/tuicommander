//! Read-only, fileless story projection into mdkb. SQLite remains authoritative.
use super::{StoryStore, store::IngestVersion};
use crate::mdkb_client::MdkbClient;
use anyhow::{Result, anyhow, bail};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::Notify;

const SOURCE: &str = "tuic-stories";
const RETRY_INTERVAL: Duration = Duration::from_secs(30);
static COMMITTED: LazyLock<Notify> = LazyLock::new(Notify::new);

pub(super) fn notify_committed() {
    COMMITTED.notify_one();
}

/// Desktop and headless hosts both own native story mutations. No daemon is
/// launched or restarted here; ingestion is optional and retries a missing one.
pub(crate) fn spawn() {
    tokio::spawn(async {
        let path = crate::db::path(crate::db::TUIC_DB);
        loop {
            if path.is_file() {
                match MdkbClient::connect().await {
                    Ok(mut client) => {
                        let db = path.clone();
                        let opened =
                            tokio::task::spawn_blocking(move || StoryStore::open_at(&db)).await;
                        match opened {
                            Ok(Ok(store)) => {
                                if let Err(error) = sync_store(Arc::new(store), &mut client).await {
                                    tracing::warn!("native story mdkb sync failed: {error:#}");
                                }
                            }
                            other => {
                                tracing::warn!("native story mdkb store unavailable: {other:?}")
                            }
                        }
                    }
                    Err(error) => tracing::debug!("native story mdkb sync deferred: {error}"),
                }
            }
            tokio::select! {
                () = COMMITTED.notified() => {},
                () = tokio::time::sleep(RETRY_INTERVAL) => {},
            }
        }
    });
}

/// Reconcile id+revision inventories on every connection. Reading the document
/// just before sending also catches mutations committed during the inventory RPC.
/// A stale in-flight write cannot overwrite a newer one (mdkb's revision check).
pub(super) async fn sync_store(store: Arc<StoryStore>, client: &mut MdkbClient) -> Result<()> {
    let inventory_store = store.clone();
    let roots = tokio::task::spawn_blocking(move || inventory_store.ingest_ids())
        .await?
        .map_err(|error| anyhow!(error))?;
    let mut failure = None;
    for (root, ids) in roots {
        // One refused root must not stop other admitted projects from syncing.
        if let Err(error) = sync_root(store.clone(), client, &root, ids).await {
            tracing::warn!(project = root, "native story mdkb push failed: {error:#}");
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

async fn sync_root(
    store: Arc<StoryStore>,
    client: &mut MdkbClient,
    root: &str,
    ids: Vec<IngestVersion>,
) -> Result<()> {
    // The durable ledger outlives checkouts. Keep it for restoration, but do not
    // send roots mdkb cannot resolve or turn removed projects into user warnings.
    if !std::path::Path::new(root).is_dir() {
        return Ok(());
    }
    let remote: HashMap<_, _> = client
        .external_list(root, SOURCE)
        .await?
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect();
    let mut failure = None;
    for version in ids {
        let id = version.id;
        if remote
            .get(&id)
            .is_some_and(|current| current.revision == version.revision)
        {
            continue;
        }
        let item_store = store.clone();
        let item_id = id.clone();
        let record = tokio::task::spawn_blocking(move || item_store.ingest_document(&item_id))
            .await?
            .map_err(|error| anyhow!(error))?;
        if let Some(current) = remote.get(&id) {
            if current.revision > record.revision {
                // Fail visibly rather than overwrite a newer owner's record after
                // restoring an older story database. Ordinary reconnects never rebase.
                failure = Some(anyhow!("mdkb has a newer revision for story {id}"));
                continue;
            }
            if current.revision == record.revision {
                if current.deleted != record.document.is_none() {
                    failure = Some(anyhow!("mdkb has a conflicting story tombstone for {id}"));
                }
                continue;
            }
        }
        let result = match record.document {
            Some(document) => {
                client
                    .external_upsert(root, SOURCE, &id, record.revision, document)
                    .await
            }
            None => {
                client
                    .external_delete(root, SOURCE, &id, record.revision)
                    .await
            }
        };
        if let Err(error) = result {
            // A rejected/oversized document must not starve sibling stories.
            tracing::warn!(story_id = id, "native story mdkb write deferred: {error:#}");
            failure = Some(error);
        }
    }
    // Unknown daemon IDs are not ours to delete: another isolated TUIC instance
    // may index the same root. Only our durable deletion ledger owns tombstones.
    if let Some(error) = failure {
        bail!(error);
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Catches: a deleted project repeatedly reaching mdkb instead of waiting for restoration.
    #[tokio::test]
    async fn ingest_missing_root_skips_rpc_until_restored() {
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir(&root).unwrap();
        let store = Arc::new(StoryStore::open_at(&dir.path().join("stories.sqlite3")).unwrap());
        let (stream, mut server) = tokio::net::UnixStream::pair().unwrap();
        let mut client = MdkbClient::from_stream(stream, Duration::from_secs(30));
        let replies = tokio::spawn(async move {
            let mut requests = Vec::new();
            loop {
                let mut header = [0; 4];
                if server.read_exact(&mut header).await.is_err() {
                    break;
                }
                let mut body = vec![0; u32::from_le_bytes(header) as usize];
                server.read_exact(&mut body).await.unwrap();
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let response = serde_json::to_vec(&serde_json::json!({
                    "id": request["id"], "result": {"documents": []}
                }))
                .unwrap();
                requests.push(request);
                server
                    .write_all(&(response.len() as u32).to_le_bytes())
                    .await
                    .unwrap();
                server.write_all(&response).await.unwrap();
            }
            requests
        });
        let root_str = root.to_str().unwrap();
        sync_root(store.clone(), &mut client, root_str, vec![])
            .await
            .unwrap();
        std::fs::remove_dir(&root).unwrap();
        for _ in 0..2 {
            sync_root(store.clone(), &mut client, root_str, vec![])
                .await
                .unwrap();
        }
        // A file at the old path is not a restored project directory.
        std::fs::write(&root, "not a directory").unwrap();
        sync_root(store.clone(), &mut client, root_str, vec![])
            .await
            .unwrap();
        std::fs::remove_file(&root).unwrap();
        std::fs::create_dir(&root).unwrap();
        sync_root(store, &mut client, root_str, vec![])
            .await
            .unwrap();
        drop(client);
        let requests = replies.await.unwrap();
        assert_eq!(
            requests.len(),
            2,
            "only existing and restored roots reach mdkb"
        );
        assert!(
            requests
                .iter()
                .all(|request| request["method"] == "external.list")
        );
    }
}
