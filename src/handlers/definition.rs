use crate::server::MarkdownLanguageServer;
use std::fs;
use std::path::Path;
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
/// This function searches for the last occurrence of "[[" before the offset and the next "]]" after it.
/// If found, it returns the virtual path portion (i.e. the text before a pipe if one exists).
fn extract_wikilink_at_offset(content: &str, offset: usize) -> Option<String> {
    let start_index = content[..offset].rfind("[[")?;
    let end_index = content[offset..].find("]]")?;
    let end_index = offset + end_index;
    // Extract the text inside the delimiters and trim it.
    let link_text = content[start_index + 2..end_index].trim();
    // If there's a pipe (|) use only the part before it as the virtual path.
    let virtual_link = if let Some(pipe_index) = link_text.find('|') {
        link_text[..pipe_index].trim()
    } else {
        link_text
    };
    Some(virtual_link.to_string())
}

/// Handles a go-to definition request for wiki-links.  
/// If the cursor is within a wiki-link (in either format), it returns a Location pointing to the corresponding file,
/// using the record’s local path.
pub async fn handle_definition(
    server: &MarkdownLanguageServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let text_doc = params.text_document_position_params.text_document.uri;
    eprintln!("[DEBUG] Definition request for document: {}", text_doc);

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

    let pos = params.text_document_position_params.position;
    eprintln!(
        "[DEBUG] Definition request position: line {}, character {}",
        pos.line, pos.character
    );
    let offset = position_to_offset(&content, &pos);
    eprintln!("[DEBUG] Computed byte offset: {}", offset);

    if let Some(virtual_link) = extract_wikilink_at_offset(&content, offset) {
        eprintln!(
            "[DEBUG] Extracted wiki-link (virtual path): '{}'",
            virtual_link
        );
        // Lookup the page record using the virtual path.
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
            // Convert the local path to a file URI.
            let def_uri =
                Url::from_file_path(&record.path).map_err(|_| tower_lsp::jsonrpc::Error {
                    code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                    message: "Could not convert local path to URI".to_string().into(),
                    data: None,
                })?;
            // Create a Location pointing to the start of the file (line 0, character 0).
            let location = Location {
                uri: def_uri,
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
            };
            return Ok(Some(GotoDefinitionResponse::Scalar(location)));
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
