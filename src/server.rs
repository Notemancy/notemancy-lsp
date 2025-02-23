// src/server.rs
use crate::handlers::workspace_symbol;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

pub struct MarkdownLanguageServer {
    pub(crate) client: Client, // Changed from private to pub(crate)
    pub(crate) db: notemancy_core::db::Database,
}

impl std::fmt::Debug for MarkdownLanguageServer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        <Client as std::fmt::Debug>::fmt(&self.client, f)
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for MarkdownLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                workspace_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // The method is actually called symbol
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        workspace_symbol::handle(self, params).await
    }
}
