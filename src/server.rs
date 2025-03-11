use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tower_lsp::jsonrpc::Error;
use tower_lsp::lsp_types::*;
use tower_lsp::{async_trait, Client, LanguageServer};

use crate::parser::parse_markdown_symbols;
use notemancy_core::utils;

// Import fuzzy matcher types.
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

#[derive(Debug)]
pub struct Backend {
    /// Cache of open documents (URI → document text).
    pub docs: Arc<Mutex<HashMap<Url, String>>>,
    pub client: Client,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            docs: Arc::new(Mutex::new(HashMap::new())),
            client,
        }
    }
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult, Error> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Enable in-document symbol support.
                document_symbol_provider: Some(OneOf::Left(true)),
                // Advertise workspace symbol support.
                workspace_symbol_provider: Some(OneOf::Left(true)),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..ServerCapabilities::default()
            },
            server_info: None,
        })
    }

    async fn shutdown(&self) -> Result<(), Error> {
        Ok(())
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Markdown LSP initialized!")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let text = params.text_document.text;
        let uri = params.text_document.uri;
        self.docs.lock().unwrap().insert(uri, text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.docs
                .lock()
                .unwrap()
                .insert(params.text_document.uri, change.text);
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>, Error> {
        let uri = params.text_document.uri;
        let docs = self.docs.lock().unwrap();
        if let Some(text) = docs.get(&uri) {
            let symbols = parse_markdown_symbols(text);
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        } else {
            Ok(None)
        }
    }

    /// Workspace symbol request:
    /// For every file in the workspace, read its content and parse markdown symbols.
    /// Then convert these into flat SymbolInformation items.
    /// If a query is provided, perform fuzzy matching (case-sensitive) against symbol names.
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>, Error> {
        let mut results = Vec::new();

        // Get all file paths (lpaths) from your core utils.
        let paths = match utils::get_all_paths(true, false).map_err(|e| e.to_string()) {
            Ok(p) => p,
            Err(err_string) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Error fetching file paths: {}", err_string),
                    )
                    .await;
                return Err(Error::internal_error());
            }
        };

        for lpath in paths {
            if let Ok(uri) = Url::from_file_path(&lpath) {
                // Read the full file content (including YAML frontmatter) so that the positions match.
                let content =
                    match utils::read_file(Some(&lpath), None, true).map_err(|e| e.to_string()) {
                        Ok(c) => c,
                        Err(err_string) => {
                            self.client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Error reading file {}: {}", lpath, err_string),
                                )
                                .await;
                            continue;
                        }
                    };

                // Parse the file content for Markdown symbols.
                let symbols = parse_markdown_symbols(&content);

                // Convert each parsed symbol into a flat SymbolInformation.
                for ds in symbols {
                    let sym_info = SymbolInformation {
                        name: ds.name,
                        kind: ds.kind,
                        location: Location {
                            uri: uri.clone(),
                            range: ds.range,
                        },
                        container_name: None,
                        deprecated: None,
                        tags: None,
                    };
                    results.push(sym_info);
                }
            }
        }

        // If a query is provided, use fuzzy matching.
        let filtered = if !params.query.is_empty() {
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(i64, SymbolInformation)> = results
                .into_iter()
                .filter_map(|s| {
                    matcher
                        .fuzzy_match(&s.name, &params.query)
                        .map(|score| (score, s))
                })
                .collect();

            // Sort by descending match score.
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored.into_iter().map(|(_, s)| s).collect()
        } else {
            results
        };

        Ok(Some(filtered))
    }
}
