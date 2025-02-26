use crate::server::MarkdownLanguageServer;
use notemancy_core::db::FileRecord;
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

/// Provides completions for wiki-links.
/// This function replaces either just the opening `[[` or a complete auto-paired `[[]]`
/// with the full wiki-link syntax: `[[<virtual path> | alias]]`
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

    // Split the document into lines and make sure the current line exists.
    let lines: Vec<&str> = text.lines().collect();
    if position.line as usize >= lines.len() {
        return Ok(None);
    }
    let current_line = lines[position.line as usize];

    // Clamp the cursor index to avoid out-of-bounds.
    let cursor_index = std::cmp::min(position.character as usize, current_line.len());

    // Find the start of the wiki-link trigger by searching backwards for "[[".
    let start_index = match current_line[..cursor_index].rfind("[[") {
        Some(idx) => idx,
        None => return Ok(None),
    };

    // Check if there is a closing "]]" immediately after the cursor.
    let end_index = if cursor_index + 2 <= current_line.len()
        && &current_line[cursor_index..cursor_index + 2] == "]]"
    {
        cursor_index + 2
    } else {
        cursor_index
    };

    // Build the text edit range.
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

    // Generate completion items.
    let mut items = Vec::new();
    let files = server.db.get_file_tree().unwrap_or_default();
    for file in files {
        if !file.virtual_path.ends_with(".md") {
            continue;
        }

        let virtual_path = get_virtual_path(&file.virtual_path);
        let alias = get_alias(&file);
        // Create the wiki-link, including the brackets.
        let insertion_text = format!("[[{} | {}]]", virtual_path, alias);

        items.push(CompletionItem {
            label: virtual_path,
            kind: Some(CompletionItemKind::FILE),
            // Use text_edit to replace the trigger region with the complete wiki-link.
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range,
                new_text: insertion_text,
            })),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        });
    }

    Ok(Some(CompletionResponse::Array(items)))
}
