use notemancy_lsp::server::MarkdownLanguageServer;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(MarkdownLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
