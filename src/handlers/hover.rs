use crate::server::MarkdownLanguageServer;
use std::fs;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

/// Convert a Position (line, character) into a byte offset in the document.
fn position_to_offset(content: &str, position: &Position) -> usize {
    let mut offset = 0;
    for (i, line) in content.lines().enumerate() {
        if i as u32 == position.line {
            offset += position.character as usize;
            break;
        } else {
            // +1 for the newline character
            offset += line.len() + 1;
        }
    }
    offset
}

/// Attempt to extract a wiki-link at the given byte offset in the document.
/// This function looks for the last occurrence of "[[" before the offset and the next occurrence of "]]" after the offset.
/// If found, it returns the virtual path portion (trimmed) even if an alias is present.
fn extract_wikilink_at_offset(content: &str, offset: usize) -> Option<String> {
    let start_index = content[..offset].rfind("[[")?;
    let end_index = content[offset..].find("]]")?;
    let end_index = offset + end_index;
    // Extract the text inside the delimiters and trim it.
    let link_text = content[start_index + 2..end_index].trim();
    // If the wiki-link contains a pipe, split on it and take the left-hand side as the virtual path.
    let virtual_link = if let Some(pipe_index) = link_text.find('|') {
        link_text[..pipe_index].trim()
    } else {
        link_text
    };
    Some(virtual_link.to_string())
}

pub async fn handle_hover(
    server: &MarkdownLanguageServer,
    params: HoverParams,
) -> Result<Option<Hover>> {
    // Log the incoming hover request URI.
    let text_doc = params.text_document_position_params.text_document.uri;
    eprintln!("[DEBUG] Hover request for document: {}", text_doc);

    let file_path = text_doc
        .to_file_path()
        .map_err(|_| tower_lsp::jsonrpc::Error {
            code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
            message: "Invalid file URI".to_string().into(),
            data: None,
        })?;
    eprintln!("[DEBUG] Resolved file path: {:?}", file_path);

    let content = fs::read_to_string(&file_path).map_err(|err| tower_lsp::jsonrpc::Error {
        code: tower_lsp::jsonrpc::ErrorCode::InternalError,
        message: format!("Failed to read file: {}", err).into(),
        data: None,
    })?;
    eprintln!("[DEBUG] File content length: {} bytes", content.len());

    let hover_pos = params.text_document_position_params.position;
    eprintln!(
        "[DEBUG] Hover position: line {}, character {}",
        hover_pos.line, hover_pos.character
    );
    let offset = position_to_offset(&content, &hover_pos);
    eprintln!("[DEBUG] Computed byte offset: {}", offset);

    if let Some(virtual_link) = extract_wikilink_at_offset(&content, offset) {
        eprintln!(
            "[DEBUG] Extracted wiki-link (virtual path): '{}'",
            virtual_link
        );
        // Use the dedicated method to retrieve the record.
        if let Some(record) = server
            .db
            .get_page_by_virtual_path(&virtual_link)
            .map_err(|err| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: format!("DB query failed: {}", err).into(),
                data: None,
            })?
        {
            eprintln!(
                "[DEBUG] Found DB record: virtual_path='{}', local path='{}'",
                record.virtual_path, record.path
            );
            if let Ok(preview_content) = fs::read_to_string(&record.path) {
                eprintln!(
                    "[DEBUG] Preview content length: {} bytes",
                    preview_content.len()
                );
                let hover = Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: preview_content,
                    }),
                    range: None,
                };
                return Ok(Some(hover));
            } else {
                eprintln!(
                    "[DEBUG] Failed to read preview content from '{}'",
                    record.path
                );
            }
        } else {
            eprintln!(
                "[DEBUG] No DB record found for virtual_path: '{}'",
                virtual_link
            );
        }
    } else {
        eprintln!("[DEBUG] No wiki-link found at offset {}", offset);
    }

    Ok(None)
}
