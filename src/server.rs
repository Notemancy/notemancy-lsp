// src/server.rs
use crate::document::Document;
use std::collections::HashMap;
use std::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct MarkdownLanguageServer {
    client: Client,
    documents: Mutex<HashMap<Url, Document>>,
}

impl MarkdownLanguageServer {
    pub fn new(client: Client) -> Self {
        MarkdownLanguageServer {
            client,
            documents: Mutex::new(HashMap::new()),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for MarkdownLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                document_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Markdown LSP initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = Document::new(params.text_document.uri.clone(), params.text_document.text);
        self.documents
            .lock()
            .unwrap()
            .insert(params.text_document.uri, document);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(document) = self
            .documents
            .lock()
            .unwrap()
            .get_mut(&params.text_document.uri)
        {
            for change in params.content_changes {
                document.update_content(change.text);
            }
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        if let Some(document) = self
            .documents
            .lock()
            .unwrap()
            .get(&params.text_document.uri)
        {
            let symbols = document.get_symbols();
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        } else {
            Ok(None)
        }
    }
}
