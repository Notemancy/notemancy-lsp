use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct MarkdownLanguageServer {
    pub(crate) client: Client,
    pub(crate) db: notemancy_core::db::Database,
    pub(crate) documents: Arc<Mutex<HashMap<Url, String>>>,
}

impl std::fmt::Debug for MarkdownLanguageServer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        <Client as std::fmt::Debug>::fmt(&self.client, f)
    }
}

impl MarkdownLanguageServer {
    // A constructor for convenience.
    pub fn new(client: Client, db: notemancy_core::db::Database) -> Self {
        Self {
            client,
            db,
            documents: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for MarkdownLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Inform the client that the server is starting up.
        self.client
            .show_message(MessageType::INFO, "Notemancy LSP: Starting up...")
            .await;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["[".to_string()]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        // Once initialization is complete, send a loaded message.
        self.client
            .show_message(MessageType::INFO, "Notemancy LSP: Loaded successfully.")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // Cache the full text of opened documents.
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents.lock().unwrap().insert(uri, text);
    }

    // Update the cached text on changes.
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // For simplicity, we assume the client sends full text updates.
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents.lock().unwrap().insert(uri, change.text);
        }
    }

    // Remove closed documents from the cache.
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.lock().unwrap().remove(&uri);
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        crate::handlers::workspace_symbol::handle(self, params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        crate::handlers::document_symbol::handle_document_symbol(self, params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        crate::handlers::definition::handle_definition(self, params).await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        crate::handlers::completion::handle_completion(self, params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        crate::handlers::hover::handle_hover(self, params).await
    }
}
