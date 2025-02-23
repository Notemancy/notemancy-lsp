use crate::handlers::document_symbol;
use crate::handlers::workspace_symbol;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct MarkdownLanguageServer {
    pub(crate) client: Client,
    pub(crate) db: notemancy_core::db::Database,
}

#[tower_lsp::async_trait]
impl LanguageServer for MarkdownLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                workspace_symbol_provider: Some(OneOf::Left(true)),
                // Enable document symbol support.
                document_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // Workspace symbols method remains unchanged.
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        workspace_symbol::handle(self, params).await
    }

    // Document symbols method for the open document.
    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        document_symbol::handle_document_symbol(self, params).await
    }
}
