use crate::server::MarkdownLanguageServer;
use std::path::Path;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

/// Computes the virtual path for a given local path.
fn get_virtual_path(local_path: &str) -> String {
    let path = Path::new(local_path);
    let virtual_path = path.to_string_lossy().replace("\\", "/");
    // if virtual_path.ends_with(".md") {
    //     virtual_path = virtual_path[..virtual_path.len() - 3].to_string();
    // }
    virtual_path
}

/// Provides completions for wiki-links.
pub async fn handle_completion(
    server: &MarkdownLanguageServer,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let _ = params;
    // (Optionally) inspect params.context or the document text to ensure we are in a wiki-link context.
    // For simplicity, we assume that when the trigger character is '[' this is a wiki-link.

    let mut items = Vec::new();

    // Get all files from the workspace. (Assuming your DB returns a collection of files with a `path` field.)
    let files = server.db.get_file_tree().unwrap_or_default();
    for file in files {
        // Only consider markdown files.
        if !file.virtual_path.ends_with(".md") {
            continue;
        }

        // Compute the full virtual path for this file.
        let virtual_path = get_virtual_path(&file.virtual_path);

        // Create a CompletionItem that, when accepted, inserts the full virtual path.
        items.push(CompletionItem {
            label: virtual_path.clone(),
            kind: Some(CompletionItemKind::FILE),
            insert_text: Some(virtual_path),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            // Optionally include detail or documentation.
            ..Default::default()
        });
    }

    Ok(Some(CompletionResponse::Array(items)))
}
