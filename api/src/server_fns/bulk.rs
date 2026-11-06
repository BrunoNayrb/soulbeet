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
}

const LIVE_KEYWORDS: &[&str] = &[
    "live", "concert", "session", "acoustic", "unplugged",
    "bootleg", "demo", "rehearsal", "tour",
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
        // Placeholder: real album-first + track fallback logic goes here
        results.push(BulkResult {
            row,
            status: "failed".into(),
            reason: Some("Not implemented yet".into()),
        });
    }

    Ok(results)
}
