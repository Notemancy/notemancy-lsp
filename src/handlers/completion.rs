use crate::server::MarkdownLanguageServer;
use notemancy_core::db::FileRecord;
use serde_json::Value;
use std::path::Path;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

/// Computes the virtual path for a given local path.
fn get_virtual_path(local_path: &str) -> String {
    let path = Path::new(local_path);
    let virtual_path = path.to_string_lossy().replace("\\", "/");
    virtual_path
}

/// Extracts an alias from the file record's metadata.  
/// If the metadata JSON contains a "title" field, that is used as the alias;  
/// otherwise, it falls back to the file name extracted from the virtual path.
fn get_alias(file: &FileRecord) -> String {
    if let Ok(val) = serde_json::from_str::<Value>(&file.metadata) {
        if let Some(title) = val.get("title").and_then(|t| t.as_str()) {
            return title.to_string();
        }
    }
    // Fallback: use the filename from the virtual path.
    let path = Path::new(&file.virtual_path);
    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        return file_name.to_string();
    }
    file.virtual_path.clone()
}

/// Provides completions for wiki-links.
pub async fn handle_completion(
    server: &MarkdownLanguageServer,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let _ = params;
    let mut items = Vec::new();

    // Get all files from the workspace.
    let files = server.db.get_file_tree().unwrap_or_default();
    for file in files {
        // Only consider markdown files.
        if !file.virtual_path.ends_with(".md") {
            continue;
        }

        // Compute the full virtual path for this file.
        let virtual_path = get_virtual_path(&file.virtual_path);
        let alias = get_alias(&file);

        // Construct the insertion text as [[virtual_path | alias]]
        let insertion_text = format!("{} | {}", virtual_path, alias);

        // Create a CompletionItem that, when accepted, inserts the full wiki-link.
        items.push(CompletionItem {
            label: virtual_path, // You may also consider using the alias as the label.
            kind: Some(CompletionItemKind::FILE),
            insert_text: Some(insertion_text),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        });
    }

    Ok(Some(CompletionResponse::Array(items)))
}
