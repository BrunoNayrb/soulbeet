use super::search::{search_album, search_track, SearchQuery};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

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
    "live", "concert", "session", "acoustic", "unplugged",
    "bootleg", "demo", "rehearsal", "tour", "festival",
];

fn is_live(s: &str) -> bool {
    let lower = s.to_lowercase();
    LIVE_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

#[post("/api/bulk/import")]
pub async fn bulk_import(
    rows: Vec<BulkRow>,
    target_folder: String,
) -> Result<Vec<BulkResult>, ServerFnError> {
    let mut results = Vec::new();

    for row in rows {
        let mut result = BulkResult {
            row: row.clone(),
            status: "failed".into(),
            reason: Some("No match found".into()),
            source: None,
        };

        // 1. Try album search first
        if let Some(album) = &row.album {
            let query = SearchQuery {
                artist: Some(row.artist.clone()),
                query: album.clone(),
                provider: None,
            };
            if let Ok(album_results) = search_album(query).await {
                if let Some(album_match) = album_results.results.first() {
                    // Found an album — now look for the specific track
                    let track_query = SearchQuery {
                        artist: Some(row.artist.clone()),
                        query: row.title.clone(),
                        provider: None,
                    };
                    if let Ok(track_results) = search_track(track_query).await {
                        if let Some(track) = track_results.results.iter().find(|t| {
                            !is_live(&t.to_string()) // crude but works for path/title
                        }) {
                            result.status = "queued".into();
                            result.reason = None;
                            result.source = Some(format!("album:{}", album_match));
                            results.push(result);
                            continue;
                        }
                    }
                }
            }
        }

        // 2. Fallback: single track search
        let track_query = SearchQuery {
            artist: Some(row.artist.clone()),
            query: row.title.clone(),
            provider: None,
        };
        if let Ok(track_results) = search_track(track_query).await {
            if let Some(track) = track_results.results.iter().find(|t| !is_live(&t.to_string())) {
                result.status = "queued".into();
                result.reason = None;
                result.source = Some("track".into());
            } else {
                result.reason = Some("Only live versions found".into());
            }
        } else {
            result.reason = Some("Search failed".into());
        }

        results.push(result);
    }

    Ok(results)
}
