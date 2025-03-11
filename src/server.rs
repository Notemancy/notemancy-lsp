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
                // Document symbols (for in-file headings)
                document_symbol_provider: Some(OneOf::Left(true)),
                // Workspace symbols (for full-note search)
                workspace_symbol_provider: Some(OneOf::Left(true)),
                // Completions for wiki-links.
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["[".to_string()]),
                    resolve_provider: Some(false),
                    completion_item: None,
                    all_commit_characters: None,
                    work_done_progress_options: Default::default(),
                }),
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
    /// If a query is provided, apply fuzzy matching (case-sensitive) on the symbol name.
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>, Error> {
        let mut results = Vec::new();

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

                let symbols = parse_markdown_symbols(&content);
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
        if !params.query.is_empty() {
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(i64, SymbolInformation)> = results
                .into_iter()
                .filter_map(|s| {
                    matcher
                        .fuzzy_match(&s.name, &params.query)
                        .map(|score| (score, s))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            let filtered = scored.into_iter().map(|(_, s)| s).collect();
            Ok(Some(filtered))
        } else {
            Ok(Some(results))
        }
    }

    /// Completion request for wiki-links:
    /// When the text preceding the cursor contains a wiki-link trigger (i.e. it contains "[["),
    /// extract the substring after the last occurrence of "[[" as the query.
    /// Then, fetch wiki-link records from the core database and filter them via fuzzy matching.
    /// The inserted text is formatted as "<vpath> | <title>".
    async fn completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>, Error> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let text = {
            let docs = self.docs.lock().unwrap();
            docs.get(&uri).cloned()
        };

        let text = match text {
            Some(t) => t,
            None => return Ok(None),
        };

        let lines: Vec<&str> = text.lines().collect();
        let current_line = if (position.line as usize) < lines.len() {
            lines[position.line as usize]
        } else {
            ""
        };

        // Get the text up to the cursor.
        let prefix = &current_line[..(position.character as usize).min(current_line.len())];

        // Find the last occurrence of "[[" in the prefix.
        let query = if let Some(idx) = prefix.rfind("[[") {
            &prefix[idx + 2..]
        } else {
            ""
        };

        // Fetch wiki-link records from the database.
        let records =
            match utils::get_records_by_column(&["vpath", "title"]).map_err(|e| e.to_string()) {
                Ok(r) => r,
                Err(err) => {
                    self.client
                        .log_message(
                            MessageType::ERROR,
                            format!("Error fetching wiki-link records: {}", err),
                        )
                        .await;
                    return Err(Error::internal_error());
                }
            };

        let mut candidates = Vec::new();
        for record in records {
            let vpath = record.get("vpath").and_then(|v| v.clone());
            let title = record.get("title").and_then(|t| t.clone());
            if let (Some(vpath), Some(title)) = (vpath, title) {
                // Prepare the text to insert: "<vpath> | <title>"
                let insert_text = format!("{} | {}", vpath, title);
                let item = CompletionItem {
                    label: title.clone(),
                    kind: Some(CompletionItemKind::FILE),
                    insert_text: Some(insert_text),
                    ..CompletionItem::default()
                };
                candidates.push(item);
            }
        }

        // If a query is provided, use fuzzy matching to filter wiki-link completions.
        let items = if !query.is_empty() {
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(i64, CompletionItem)> = candidates
                .into_iter()
                .filter_map(|item| {
                    if let Some(score) = matcher.fuzzy_match(&item.label, query) {
                        Some((score, item))
                    } else {
                        None
                    }
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored.into_iter().map(|(_, item)| item).collect()
        } else {
            candidates
        };

        Ok(Some(CompletionResponse::Array(items)))
    }
}
