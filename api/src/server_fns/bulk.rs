use super::search::{search_album, search_track, SearchQuery};
use crate::AuthSession;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
use std::path::PathBuf;
#[cfg(feature = "server")]
use std::time::{Duration, Instant};

#[cfg(feature = "server")]
use shared::download::{
    DownloadEvent, DownloadProgress, DownloadState, DownloadableGroup, DownloadableItem,
    EnqueueStatus, SearchState,
};
#[cfg(feature = "server")]
use shared::metadata::Track;

const HISTORY_PATH: &str = "/data/bulk_history.json";
const HISTORY_LIMIT: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkRow {
    pub title: String,
    pub album: Option<String>,
    pub artist: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkResult {
    pub row: BulkRow,
    pub status: String,
    pub reason: Option<String>,
    pub source: Option<String>,
}

const LIVE_KEYWORDS: &[&str] = &[
    "live", "concert", "session", "acoustic", "unplugged", "bootleg", "demo", "rehearsal",
    "tour", "festival",
];

fn is_live(s: &str) -> bool {
    s.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| LIVE_KEYWORDS.contains(&word))
}

fn loose(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace())
        .collect()
}

fn label(result: &shared::metadata::SearchResult) -> String {
    match result {
        shared::metadata::SearchResult::Track(track) => format!(
            "{} {} {}",
            track.title,
            track.album_title.as_deref().unwrap_or(""),
            track.artist
        ),
        shared::metadata::SearchResult::Album(album) => format!("{} {}", album.title, album.artist),
    }
}

#[post("/api/bulk/import", auth: AuthSession)]
pub async fn bulk_import(
    rows: Vec<BulkRow>,
    target_folder: String,
) -> Result<Vec<BulkResult>, ServerFnError> {
    let username = auth.0.username.clone();
    let user_id = auth.0.sub.clone();
    let pending: Vec<BulkResult> = rows
        .iter()
        .cloned()
        .map(|row| BulkResult {
            row,
            status: "working".into(),
            reason: Some("Searching".into()),
            source: None,
        })
        .collect();
    remember(&pending).await;

    #[cfg(feature = "server")]
    {
        let rows = rows;
        let target_folder = target_folder;
        tokio::spawn(async move {
            run_import(rows, target_folder, username, user_id).await;
        });
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (rows, target_folder, username, user_id);
    }

    Ok(read_history().await)
}

#[post("/api/bulk/history", auth: AuthSession)]
pub async fn bulk_history() -> Result<Vec<BulkResult>, ServerFnError> {
    Ok(read_history().await)
}

#[post("/api/bulk/history/clear", auth: AuthSession)]
pub async fn clear_bulk_history() -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        let _guard = history_lock().lock().await;
        let _ = tokio::fs::remove_file(HISTORY_PATH).await;
    }
    Ok(())
}

#[cfg(feature = "server")]
fn history_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[cfg(feature = "server")]
async fn run_import(rows: Vec<BulkRow>, target_folder: String, username: String, user_id: String) {
    for row in rows {
        let result = process_row(&row, &target_folder, &username, &user_id).await;
        replace_working(&result).await;
    }
}

#[cfg(feature = "server")]
async fn process_row(
    row: &BulkRow,
    target_folder: &str,
    username: &str,
    user_id: &str,
) -> BulkResult {
    let mut result = BulkResult {
        row: row.clone(),
        status: "failed".into(),
        reason: Some("No match found".into()),
        source: None,
    };

    let mut matched = false;

    if let Some(album) = &row.album {
        let query = SearchQuery {
            artist: Some(row.artist.clone()),
            query: album.clone(),
            provider: None,
        };
        if let Ok(album_results) = search_album(query).await {
            if album_results
                .results
                .iter()
                .any(|album_match| !is_live(&label(album_match)))
            {
                let track_query = SearchQuery {
                    artist: Some(row.artist.clone()),
                    query: row.title.clone(),
                    provider: None,
                };
                if let Ok(track_results) = search_track(track_query).await {
                    if track_results
                        .results
                        .iter()
                        .any(|track| !is_live(&label(track)))
                    {
                        matched = true;
                    } else if !track_results.results.is_empty() {
                        result.reason = Some("Only live versions found".into());
                    }
                }
            }
        }
    }

    if !matched && result.reason.as_deref() != Some("Only live versions found") {
        let track_query = SearchQuery {
            artist: Some(row.artist.clone()),
            query: row.title.clone(),
            provider: None,
        };
        if let Ok(track_results) = search_track(track_query).await {
            if track_results
                .results
                .iter()
                .any(|track| !is_live(&label(track)))
            {
                matched = true;
            } else if !track_results.results.is_empty() {
                result.reason = Some("Only live versions found".into());
            }
        } else {
            result.reason = Some("Search failed".into());
        }
    }

    if matched {
        match queue_matched(row, target_folder, username, user_id).await {
            Ok(source) => {
                result.status = "queued".into();
                result.reason = None;
                result.source = Some(source);
            }
            Err(reason) => {
                result.status = "failed".into();
                result.reason = Some(reason);
                result.source = None;
            }
        }
    }

    result
}

#[cfg(feature = "server")]
async fn remember(new_rows: &[BulkResult]) {
    if new_rows.is_empty() {
        return;
    }
    let _guard = history_lock().lock().await;
    let mut all = new_rows.to_vec();
    all.extend(read_history_inner().await);
    all.truncate(HISTORY_LIMIT);
    write_history_inner(&all).await;
}

#[cfg(not(feature = "server"))]
async fn remember(_new_rows: &[BulkResult]) {}

#[cfg(feature = "server")]
async fn replace_working(updated: &BulkResult) {
    let _guard = history_lock().lock().await;
    let mut all = read_history_inner().await;
    if let Some(slot) = all.iter_mut().find(|item| {
        item.status == "working"
            && item.row.title == updated.row.title
            && item.row.artist == updated.row.artist
            && item.row.album == updated.row.album
    }) {
        *slot = updated.clone();
    }
    write_history_inner(&all).await;
}

#[cfg(feature = "server")]
async fn read_history() -> Vec<BulkResult> {
    let _guard = history_lock().lock().await;
    read_history_inner().await
}

#[cfg(not(feature = "server"))]
async fn read_history() -> Vec<BulkResult> {
    Vec::new()
}

#[cfg(feature = "server")]
async fn read_history_inner() -> Vec<BulkResult> {
    match tokio::fs::read_to_string(HISTORY_PATH).await {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

#[cfg(feature = "server")]
async fn write_history_inner(rows: &[BulkResult]) {
    if let Ok(text) = serde_json::to_string_pretty(rows) {
        let _ = tokio::fs::write(HISTORY_PATH, text).await;
    }
}

#[cfg(feature = "server")]
async fn queue_matched(
    row: &BulkRow,
    target_folder: &str,
    username: &str,
    user_id: &str,
) -> Result<String, String> {
    use crate::globals::{get_or_create_user_channel, register_user_task, unregister_user_task};
    use crate::models::folder::Folder;
    use crate::server_fns::download::enqueue::{deliver, enqueue_request, stopped_rows, Watcher};
    use crate::server_fns::download::monitor::{DownloadMonitor, WatchedTransfer};
    use crate::services::download_backend;
    use crate::transfer_registry::{BatchRef, TransferOwner};

    let folder = Folder::find_for_user_by_path(user_id, target_folder)
        .await
        .map_err(|e| e)?
        .ok_or_else(|| {
            format!("Folder not found: {target_folder}. Add that exact path in Soulbeet settings.")
        })?;
    tokio::fs::create_dir_all(&folder.path)
        .await
        .map_err(|e| format!("Could not create {target_folder}: {e}"))?;

    let backend = download_backend(None).await.map_err(|e| e)?;
    let track = Track {
        id: String::new(),
        title: row.title.clone(),
        artist: row.artist.clone(),
        album_id: None,
        album_title: row.album.clone(),
        release_date: None,
        duration: None,
        mbid: None,
        release_mbid: None,
    };
    let search_id = backend
        .start_search(None, std::slice::from_ref(&track))
        .await
        .map_err(|e| format!("slskd search failed: {e}"))?;

    let deadline = Instant::now() + Duration::from_secs(25);
    let mut latest = None;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let polled = backend
            .poll_search(&search_id)
            .await
            .map_err(|e| format!("slskd poll failed: {e}"))?;
        let done = matches!(
            polled.state,
            SearchState::Completed | SearchState::NotFound | SearchState::TimedOut
        );
        latest = Some(polled);
        if done || Instant::now() >= deadline {
            break;
        }
    }

    let groups = latest.map(|result| result.groups).unwrap_or_default();
    let picked = pick_file(&groups, &row.title)
        .ok_or_else(|| "No matching non-live file on slskd".to_string())?;
    let source = format!("{} ({})", picked.title, picked.quality);

    let fresh = BatchRef::new(
        TransferOwner {
            user_id: user_id.to_string(),
            batch_id: uuid::Uuid::new_v4().to_string(),
        },
        row.title.clone(),
        PathBuf::from(&folder.path),
    );
    let enq = enqueue_request(&backend, vec![picked], fresh).await;
    if let Some(err) = &enq.error {
        return Err(format!("Could not queue download: {err:?}"));
    }

    let batch = enq.batch;
    let results = enq.results;
    let watcher = enq.watcher;
    let (tx, _) = get_or_create_user_channel(username).await;
    let enqueued: Vec<_> = results
        .iter()
        .filter(|row| {
            matches!(
                row.status,
                EnqueueStatus::Enqueued | EnqueueStatus::AlreadyInFlight
            )
        })
        .cloned()
        .collect();

    if enqueued.is_empty() {
        let why = results
            .iter()
            .find_map(|row| match &row.status {
                EnqueueStatus::Failed(reason) => Some(reason.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "Download was not queued".into());
        return Err(why);
    }

    let queued_rows: Vec<_> = enqueued
        .iter()
        .map(|row| DownloadProgress {
            id: row.id.clone(),
            source: row.source.clone(),
            item: row.item.clone(),
            size: row.size,
            transferred: 0,
            state: DownloadState::Queued,
            percent: 0.0,
            speed: 0.0,
            error: None,
            backend: Some(backend.id().to_string()),
            batch_id: Some(batch.owner.batch_id.clone()),
            batch_label: Some(batch.label.clone()),
            local_path: None,
            import_output: None,
            retryable: false,
            can_choose_import: false,
        })
        .collect();
    let _ = tx.send(DownloadEvent::Progress(queued_rows));

    match watcher {
        Watcher::Joined(reservation) => {
            let lost = deliver(reservation, &enqueued, &backend, &batch).await;
            if !lost.is_empty() {
                let _ = tx.send(DownloadEvent::Progress(stopped_rows(&lost, &batch)));
            }
        }
        Watcher::Own if enqueued.is_empty() && batch.inbox.close_if_idle() => {}
        Watcher::Own => {
            let transfers: Vec<WatchedTransfer> =
                enqueued.iter().map(WatchedTransfer::from).collect();
            let task_username = username.to_string();
            let task_cancellation = register_user_task(username).await;
            tokio::spawn(async move {
                let mut monitor = DownloadMonitor::new(
                    transfers,
                    batch.target.clone(),
                    tx,
                    task_cancellation,
                    task_username.clone(),
                    batch,
                );
                monitor.run().await;
                unregister_user_task(&task_username).await;
            });
        }
    }

    Ok(source)
}

#[cfg(not(feature = "server"))]
async fn queue_matched(
    _row: &BulkRow,
    _target_folder: &str,
    _username: &str,
    _user_id: &str,
) -> Result<String, String> {
    Err("server only".into())
}

#[cfg(feature = "server")]
fn pick_file(groups: &[DownloadableGroup], wanted_title: &str) -> Option<DownloadableItem> {
    let wanted = loose(wanted_title);
    let mut best: Option<(f64, DownloadableItem)> = None;
    for group in groups {
        let group_label = format!("{} {}", group.title, group.artist.as_deref().unwrap_or(""));
        if is_live(&group_label) {
            continue;
        }
        for item in &group.items {
            let item_label = format!("{} {} {}", item.title, item.album, item.artist);
            if is_live(&item_label) {
                continue;
            }
            let title = loose(&item.title);
            if !title.contains(&wanted) && !wanted.contains(&title) {
                continue;
            }
            let mut score = item.quality_score;
            if item.quality.to_lowercase().contains("flac") {
                score += 1.0;
            }
            let replace = best
                .as_ref()
                .map(|(current, _)| score > *current)
                .unwrap_or(true);
            if replace {
                best = Some((score, item.clone()));
            }
        }
    }
    best.map(|(_, item)| item)
}
