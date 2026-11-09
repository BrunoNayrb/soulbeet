use dioxus::prelude::*;
use api::server_fns::bulk::{bulk_import, BulkRow, BulkResult};

#[component]
pub fn BulkPage() -> Element {
    let mut csv_text = use_signal(|| String::new());
    let mut results = use_signal(|| Vec::<BulkResult>::new());
    let mut loading = use_signal(|| false);

    let submit = move |_| {
        let text = csv_text.read().clone();
        loading.set(true);
        spawn(async move {
            let rows: Vec<BulkRow> = text
                .lines()
                .skip(1) // skip header
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        Some(BulkRow {
                            title: parts[0].trim().to_string(),
                            album: parts.get(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                            artist: parts.get(2).unwrap_or(&"").trim().to_string(),
                        })
                    } else {
                        None
                    }
                })
                .collect();

            match bulk_import(rows, "/share/Media/Music/Bryan".to_string()).await {
                Ok(res) => results.set(res),
                Err(e) => {
                    results.set(vec![BulkResult {
                        row: BulkRow { title: "Error".into(), album: None, artist: String::new() },
                        status: "error".into(),
                        reason: Some(e.to_string()),
                        source: None,
                    }]);
                }
            }
            loading.set(false);
        });
    };

    rsx! {
        div { class: "p-6 max-w-4xl mx-auto",
            h1 { class: "text-2xl font-bold mb-4", "Bulk CSV Import" }
            p { class: "mb-4 text-gray-400", "Paste a CSV with columns: title,album,artist" }
            textarea {
                class: "w-full h-48 p-3 bg-gray-800 border border-gray-700 rounded text-sm font-mono",
                placeholder: "title,album,artist\nAPT,APT,ROSÉ, Bruno Mars\n1985,1985,Bowling for Soup",
                value: "{csv_text}",
                oninput: move |e| csv_text.set(e.value()),
            }
            button {
                class: "mt-3 px-4 py-2 bg-purple-600 hover:bg-purple-700 rounded",
                disabled: loading(),
                onclick: submit,
                if loading() { "Processing..." } else { "Import" }
            }
            if !results().is_empty() {
                div { class: "mt-6",
                    h2 { class: "text-xl font-semibold mb-2", "Results" }
                    table { class: "w-full text-sm",
                        thead {
                            tr { class: "text-left border-b border-gray-700",
                                th { "Title" }
                                th { "Artist" }
                                th { "Status" }
                                th { "Source / Reason" }
                            }
                        }
                        tbody {
                            for result in results() {
                                tr { class: "border-b border-gray-800",
                                    td { "{result.row.title}" }
                                    td { "{result.row.artist}" }
                                    td { class: if result.status == "queued" { "text-green-400" } else { "text-red-400" }, "{result.status}" }
                                    td { "{result.source.as_deref().or(result.reason.as_deref()).unwrap_or(\"-\")}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
