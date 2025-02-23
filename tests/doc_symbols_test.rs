use tower_lsp::lsp_types::*;
use tower_lsp::LanguageServer;
use tower_lsp::LspService;

extern crate notemancy_lsp;
use notemancy_lsp::server::MarkdownLanguageServer;

#[tokio::test]
async fn test_document_symbols_integration() {
    // Create a mock LSP service.
    let (service, _messages) = LspService::new(MarkdownLanguageServer::new);
    let server = service.inner();

    // Call initialize and initialized to simulate a full client connection.
    let init_params = InitializeParams::default();
    let _init_result = server.initialize(init_params).await.unwrap();
    server.initialized(InitializedParams {}).await;

    // Test document opening and symbol extraction.
    let uri = Url::parse("file:///test.md").unwrap();
    let text = "\
# Main Header
## Section 1
Some content
### Subsection 1.1
## Section 2
Content here
### Subsection 2.1
#### Deep section"
        .to_string();

    let open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: uri.clone(),
            language_id: "markdown".to_string(),
            version: 1,
            text,
        },
    };

    server.did_open(open_params).await;

    let symbol_params = DocumentSymbolParams {
        text_document: TextDocumentIdentifier { uri },
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    };

    if let Ok(Some(DocumentSymbolResponse::Nested(symbols))) =
        server.document_symbol(symbol_params).await
    {
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Main Header");

        let sections = symbols[0].children.as_ref().unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "Section 1");
        assert_eq!(sections[1].name, "Section 2");

        let subsections1 = sections[0].children.as_ref().unwrap();
        assert_eq!(subsections1.len(), 1);
        assert_eq!(subsections1[0].name, "Subsection 1.1");

        let subsections2 = sections[1].children.as_ref().unwrap();
        assert_eq!(subsections2.len(), 1);
        assert_eq!(subsections2[0].name, "Subsection 2.1");

        let deep = subsections2[0].children.as_ref().unwrap();
        assert_eq!(deep.len(), 1);
        assert_eq!(deep[0].name, "Deep section");
    } else {
        panic!("Failed to get document symbols");
    }
}
