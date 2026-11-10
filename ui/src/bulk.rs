use dioxus::prelude::*;
use api::server_fns::bulk::{
    bulk_history, bulk_import, clear_bulk_history, BulkResult, BulkRow,
};

#[component]
pub fn BulkPage() -> Element {
    let mut csv_text = use_signal(String::new);
    let mut results = use_signal(Vec::<BulkResult>::new);
    let mut loading = use_signal(|| false);
    let mut loaded = use_signal(|| false);

    use_effect(move || {
        if loaded() {
            return;
        }
        loaded.set(true);
        spawn(async move {
            if let Ok(rows) = bulk_history().await {
                results.set(rows);
            }
        });
    });

    let submit = move |_| {
        let text = csv_text.read().clone();
        loading.set(true);
        spawn(async move {
            let rows: Vec<BulkRow> = text
                .lines()
                .skip(1)
                .filter_map(|line| {
                    let parts: Vec<&str> = line.splitn(3, ',').collect();
                    if parts.len() >= 2 {
                        Some(BulkRow {
                            title: parts[0].trim().to_string(),
                            album: parts
                                .get(1)
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty()),
                            artist: parts.get(2).unwrap_or(&"").trim().to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();

            match bulk_import(rows, "/share/Media/Music/Bryan".to_string()).await {
                Ok(res) => {
                    let mut all = res;
                    all.extend(results.read().clone());
                    results.set(all);
                }
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
            textarea {
                class: "w-full h-64 min-h-40 p-3 bg-gray-800 border border-gray-700 rounded text-sm font-mono resize",
                placeholder: "title,album,artist\nAlive,Ten,Pearl Jam",
                value: "{csv_text}",
                oninput: move |e| csv_text.set(e.value()),
            }
            div { class: "mt-3 flex gap-3",
                button {
                    class: "px-4 py-2 bg-purple-600 hover:bg-purple-700 rounded disabled:opacity-50",
                    disabled: loading(),
                    onclick: submit,
                    if loading() { "Importing..." } else { "Import" }
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
                            }
                        }
                        tbody {
                            for result in results() {
                                {
                                    let status_color = if result.status == "queued" { "text-green-400" } else { "text-red-400" };
                                    let detail = result.source.as_deref().or(result.reason.as_deref()).unwrap_or("-");
                                    rsx! {
                                        tr { class: "border-b border-gray-800",
                                            td { class: "px-3 py-2", "{result.row.title}" }
                                            td { class: "px-3 py-2", "{result.row.artist}" }
                                            td { class: "px-3 py-2 {status_color}", "{result.status}" }
                                            td { class: "px-3 py-2", "{detail}" }
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
