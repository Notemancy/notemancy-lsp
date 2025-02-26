// src/main.rs
mod handlers;
mod server;
mod symbol;

use notemancy_core::db::Database;
use server::MarkdownLanguageServer;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing/logging if needed
    // tracing_subscriber::fmt().init();

    // Initialize the database
    let db = Database::new()?;

    // Create LSP server instance
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    // Use the new constructor that initializes the documents cache.
    let (service, socket) =
        LspService::build(|client| MarkdownLanguageServer::new(client, db)).finish();

    // Start the LSP server
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
