use crate::server::MarkdownLanguageServer;
use notemancy_core::db::FileRecord;
use rayon::prelude::*;
use serde_json::Value;
use std::path::Path;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

/// Computes the virtual path for a given local path.
fn get_virtual_path(local_path: &str) -> String {
    let path = Path::new(local_path);
    path.to_string_lossy().replace("\\", "/")
}

/// Extracts an alias from the file record's metadata.
fn get_alias(file: &FileRecord) -> String {
    if let Ok(val) = serde_json::from_str::<Value>(&file.metadata) {
        if let Some(title) = val.get("title").and_then(|t| t.as_str()) {
            return title.to_string();
        }
    }
    let path = Path::new(&file.virtual_path);
    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        return file_name.to_string();
    }
    file.virtual_path.clone()
}

/// Provides completions for wiki-links using Rayon for parallel processing.
/// It replaces the region from the starting "[[" to any auto-paired "]]" with the full link.
pub async fn handle_completion(
    server: &MarkdownLanguageServer,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    // Retrieve document URI and cursor position.
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    // Get the document text from the server's cache.
    let documents = server.documents.lock().unwrap();
    let text = match documents.get(&uri) {
        Some(txt) => txt,
        None => return Ok(None),
    };

    let lines: Vec<&str> = text.lines().collect();
    if position.line as usize >= lines.len() {
        return Ok(None);
    }
    let current_line = lines[position.line as usize];

    // Clamp cursor index to prevent out-of-bound errors.
    let cursor_index = std::cmp::min(position.character as usize, current_line.len());

    // Find the start of the wiki-link trigger by searching backwards for "[[".
    let start_index = match current_line[..cursor_index].rfind("[[") {
        Some(idx) => idx,
        None => return Ok(None),
    };

    // Check for an auto-paired closing bracket after the cursor.
    let end_index = if cursor_index + 2 <= current_line.len()
        && &current_line[cursor_index..cursor_index + 2] == "]]"
    {
        cursor_index + 2
    } else {
        cursor_index
    };

    let range = Range {
        start: Position {
            line: position.line,
            character: start_index as u32,
        },
        end: Position {
            line: position.line,
            character: end_index as u32,
        },
    };

    // Retrieve the file tree from the database.
    let files = server.db.get_file_tree().unwrap_or_default();

    // Use Rayon to parallelize the processing of files.
    let items: Vec<CompletionItem> = files
        .par_iter()
        .filter_map(|file| {
            if !file.virtual_path.ends_with(".md") {
                return None;
            }

            let virtual_path = get_virtual_path(&file.virtual_path);
            let alias = get_alias(file);
            let insertion_text = format!("[[{} | {}]]", virtual_path, alias);

            Some(CompletionItem {
                label: virtual_path,
                kind: Some(CompletionItemKind::FILE),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: insertion_text,
                })),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            })
        })
        .collect();

    Ok(Some(CompletionResponse::Array(items)))
}
