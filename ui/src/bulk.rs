use dioxus::prelude::*;
use api::models::folder::Folder;
use api::server_fns::bulk::{
    bulk_history, bulk_import, clear_bulk_history, BulkResult, BulkRow,
};
use api::server_fns::folder::get_user_folders;
use crate::{SearchGo, SearchPrefill};
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::JsCast;

#[component]
pub fn BulkPage() -> Element {
    let mut csv_text = use_signal(String::new);
    let mut results = use_signal(Vec::<BulkResult>::new);
    let mut loading = use_signal(|| false);
    let mut loaded = use_signal(|| false);
    let mut polling = use_signal(|| false);
    let mut folders = use_signal(Vec::<Folder>::new);
    let mut selected_path = use_signal(String::new);
    let search_prefill = try_use_context::<SearchPrefill>();
    let search_go = try_use_context::<SearchGo>();

    use_effect(move || {
        if loaded() {
            return;
        }
        loaded.set(true);
        spawn(async move {
            if let Ok(rows) = bulk_history().await {
                results.set(rows);
            }
            if let Ok(found) = get_user_folders().await {
                if selected_path.read().is_empty() {
                    if let Some(first) = found.first() {
                        selected_path.set(first.path.clone());
                    }
                }
                folders.set(found);
            }
        });
    });

    use_effect(move || {
        let still_open = results
            .read()
            .iter()
            .any(|row| row.status == "working" || row.status == "queued");
        if !still_open || polling() {
            return;
        }
        polling.set(true);
        spawn(async move {
            TimeoutFuture::new(5000).await;
            if let Ok(rows) = bulk_history().await {
                results.set(rows);
            }
            polling.set(false);
        });
    });

    let submit = move |_| {
        let text = csv_text.read().clone();
        let target = selected_path.read().clone();
        loading.set(true);
        spawn(async move {
            if target.is_empty() {
                let mut all = results.read().clone();
                all.insert(
                    0,
                    BulkResult {
                        row: BulkRow {
                            title: "Error".into(),
                            album: None,
                            artist: String::new(),
                        },
                        status: "error".into(),
                        reason: Some("Add a music folder in Settings before importing.".into()),
                        source: None,
                    },
                );
                results.set(all);
                loading.set(false);
                return;
            }

            let rows: Vec<BulkRow> = text
                .lines()
                .skip(1)
                .filter_map(|line| {
                    let parts = parse_csv_line(line);
                    if parts.len() >= 2 && !parts[0].is_empty() {
                        Some(BulkRow {
                            title: parts[0].clone(),
                            album: parts.get(1).cloned().filter(|s| !s.is_empty()),
                            artist: parts.get(2).cloned().unwrap_or_default(),
                        })
                    } else {
                        None
                    }
                })
                .collect();

            match bulk_import(rows, target).await {
                Ok(res) => results.set(res),
                Err(e) => {
                    let mut all = results.read().clone();
                    all.insert(
                        0,
                        BulkResult {
                            row: BulkRow {
                                title: "Error".into(),
                                album: None,
                                artist: String::new(),
                            },
                            status: "error".into(),
                            reason: Some(e.to_string()),
                            source: None,
                        },
                    );
                    results.set(all);
                }
            }
            loading.set(false);
        });
    };

    let clear = move |_| {
        spawn(async move {
            let _ = clear_bulk_history().await;
            results.set(Vec::new());
        });
    };

    rsx! {
        div { class: "p-6 max-w-7xl mx-auto w-full",
            h1 { class: "text-2xl font-bold mb-4", "Bulk CSV Import" }
            p { class: "mb-4 text-gray-400", "Paste a CSV with columns: title,album,artist. The first line is skipped." }
            if folders().is_empty() {
                p { class: "mb-4 text-yellow-400", "Add a music folder in Settings before importing." }
            } else {
                label { class: "block mb-4 text-sm text-gray-300",
                    "Library"
                    select {
                        class: "mt-1 w-full p-2 bg-gray-800 border border-gray-700 rounded",
                        value: "{selected_path}",
                        onchange: move |event| selected_path.set(event.value()),
                        for folder in folders() {
                            option { value: "{folder.path}", "{folder.name} — {folder.path}" }
                        }
                    }
                }
            }
            textarea {
                class: "w-full h-64 min-h-40 p-3 bg-gray-800 border border-gray-700 rounded text-sm font-mono resize",
                placeholder: "title,album,artist\nAlive,Ten,Pearl Jam",
                value: "{csv_text}",
                oninput: move |e| csv_text.set(e.value()),
            }
            div { class: "mt-3 flex gap-3",
                button {
                    class: "px-4 py-2 bg-purple-600 hover:bg-purple-700 rounded disabled:opacity-50",
                    disabled: loading() || selected_path().is_empty(),
                    onclick: submit,
                    if loading() { "Importing..." } else { "Import" }
                }
                button {
                    class: "px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded disabled:opacity-50",
                    disabled: results().iter().all(|row| row.status != "failed"),
                    onclick: move |_| download_failed_csv(results.read().as_slice()),
                    "Export failed"
                }
                button {
                    class: "px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded",
                    onclick: clear,
                    "Clear history"
                }
            }
            if !results().is_empty() {
                div { class: "mt-6 overflow-x-auto",
                    h2 { class: "text-xl font-semibold mb-2", "History" }
                    table { class: "w-full text-sm",
                        thead {
                            tr { class: "text-left border-b border-gray-700",
                                th { class: "px-3 py-2", "Title" }
                                th { class: "px-3 py-2", "Artist" }
                                th { class: "px-3 py-2", "Status" }
                                th { class: "px-3 py-2", "Source / Reason" }
                                th { class: "px-3 py-2", "" }
                            }
                        }
                        tbody {
                            for result in results() {
                                {
                                    let status_color = match result.status.as_str() {
                                        "queued" => "text-green-400",
                                        "working" => "text-yellow-400",
                                        _ => "text-red-400",
                                    };
                                    let detail = result.source.as_deref().or(result.reason.as_deref()).unwrap_or("-");
                                    let failed = result.status != "queued" && result.status != "working";
                                    let artist = result.row.artist.clone();
                                    let album = result.row.album.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| result.row.title.clone());
                                    let mut prefill = search_prefill;
                                    let mut go = search_go;
                                    rsx! {
                                        tr { class: "border-b border-gray-800",
                                            td { class: "px-3 py-2", "{result.row.title}" }
                                            td { class: "px-3 py-2", "{result.row.artist}" }
                                            td { class: "px-3 py-2 {status_color}", "{result.status}" }
                                            td { class: "px-3 py-2", "{detail}" }
                                            td { class: "px-3 py-2",
                                                if failed {
                                                    button {
                                                        class: "px-2 py-1 text-xs bg-gray-700 hover:bg-gray-600 rounded",
                                                        onclick: move |_| {
                                                            if let Some(mut prefill) = prefill {
                                                                (prefill.0).set(Some((artist.clone(), album.clone())));
                                                            }
                                                            if let Some(mut go) = go {
                                                                (go.0).set(true);
                                                            }
                                                        },
                                                        "Search album"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}


fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            fields.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    fields.push(cur.trim().to_string());
    fields
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn repair_split_title(row: &BulkRow) -> BulkRow {
    let title = row.title.as_str();
    let album = row.album.as_deref().unwrap_or("");
    let artist = row.artist.as_str();
    if title.starts_with('"') && album.ends_with('"') && artist.starts_with('"') {
        let rest = parse_csv_line(artist);
        if rest.len() >= 2 && !rest[0].is_empty() {
            let title_body = title.trim_start_matches('"');
            let album_body = album.trim_end_matches('"').trim();
            return BulkRow {
                title: format!("{title_body}, {album_body}"),
                album: Some(rest[0].clone()),
                artist: rest[1].clone(),
            };
        }
    }
    row.clone()
}

fn failed_csv(results: &[BulkResult]) -> String {
    let mut out = String::from("title,album,artist\n");
    for result in results.iter().filter(|row| row.status == "failed") {
        let row = repair_split_title(&result.row);
        let album = row.album.as_deref().unwrap_or("");
        out.push_str(&csv_field(&row.title));
        out.push(',');
        out.push_str(&csv_field(album));
        out.push(',');
        out.push_str(&csv_field(&row.artist));
        out.push('\n');
    }
    out
}

fn download_failed_csv(results: &[BulkResult]) {
    let contents = failed_csv(results);
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Ok(link) = document.create_element("a") else {
        return;
    };
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(&contents));
    let mut bag = web_sys::BlobPropertyBag::new();
    bag.set_type("text/csv;charset=utf-8");
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &bag) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if link.set_attribute("href", &url).is_err() || link.set_attribute("download", "failed-tracks.csv").is_err() {
        let _ = web_sys::Url::revoke_object_url(&url);
        return;
    }
    let Some(body) = document.body() else {
        let _ = web_sys::Url::revoke_object_url(&url);
        return;
    };
    if body.append_child(&link).is_err() {
        let _ = web_sys::Url::revoke_object_url(&url);
        return;
    }
    let Ok(link) = link.dyn_into::<web_sys::HtmlElement>() else {
        let _ = web_sys::Url::revoke_object_url(&url);
        return;
    };
    link.click();
    let _ = body.remove_child(&link);
    let _ = web_sys::Url::revoke_object_url(&url);
}
