mod parser;
mod server;

use tokio::io::{stdin, stdout};
use tower_lsp::LspService;
use tower_lsp::Server;

#[tokio::main]
async fn main() {
    let stdin = stdin();
    let stdout = stdout();
    let (service, socket) = LspService::new(|client| server::Backend::new(client));
    // Note: We now pass the service to the `serve` method.
    Server::new(stdin, stdout, socket).serve(service).await;
}
