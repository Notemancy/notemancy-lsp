use crate::server::MarkdownLanguageServer;
use crate::symbol::markdown::MarkdownSymbolExtractor;
use std::fs;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle_document_symbol(
    server: &MarkdownLanguageServer,
    params: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri;
    let file_path = uri.to_file_path().map_err(|_| tower_lsp::jsonrpc::Error {
        code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
        message: "Invalid file URI".to_string().into(),
        data: None,
    })?;

    let content = fs::read_to_string(&file_path).map_err(|err| tower_lsp::jsonrpc::Error {
        code: tower_lsp::jsonrpc::ErrorCode::InternalError,
        message: format!("Internal error reading file: {}", err).into(),
        data: None,
    })?;

    let symbols = MarkdownSymbolExtractor::extract_symbols(&content, file_path.to_str().unwrap())
        .map_err(|err| tower_lsp::jsonrpc::Error {
        code: tower_lsp::jsonrpc::ErrorCode::InternalError,
        message: format!("Internal error extracting symbols: {}", err).into(),
        data: None,
    })?;

    let doc_symbols = symbols
        .into_iter()
        .map(|s| DocumentSymbol {
            name: s.name,
            detail: None,
            kind: s.kind,
            range: s.location.range,
            selection_range: s.location.range,
            children: None,
            deprecated: None, // Added missing field.
            tags: None,       // Added missing field.
        })
        .collect::<Vec<DocumentSymbol>>();

    Ok(Some(DocumentSymbolResponse::Nested(doc_symbols)))
}
