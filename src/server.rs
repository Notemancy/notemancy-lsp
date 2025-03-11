use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tower_lsp::jsonrpc::Error;
use tower_lsp::lsp_types::*;
use tower_lsp::{async_trait, Client, LanguageServer};

use crate::parser::parse_markdown_symbols;
use notemancy_core::ai::autotag::generate_tags;
use notemancy_core::utils;

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

/// Helper function that updates (or creates) YAML frontmatter with a `tags:` field.
fn update_document_with_tags(text: &str, tags: &[String]) -> String {
    let formatted_tags = tags
        .iter()
        .map(|t| format!("\"{}\"", t))
        .collect::<Vec<_>>()
        .join(", ");
    let tag_line = format!("tags: [{}]", formatted_tags);

    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    if lines.first().map(|s| s.trim()) == Some("---") {
        if let Some(end_idx) = lines
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, l)| l.trim() == "---")
            .map(|(i, _)| i)
        {
            let mut found = false;
            for i in 1..end_idx {
                if lines[i].trim_start().starts_with("tags:") {
                    lines[i] = tag_line.clone();
                    found = true;
                    break;
                }
            }
            if !found {
                lines.insert(end_idx, tag_line.clone());
            }
        } else {
            let mut new_frontmatter = vec!["---".to_string(), tag_line.clone(), "---".to_string()];
            new_frontmatter.push(String::new());
            new_frontmatter.extend(lines);
            return new_frontmatter.join("\n");
        }
        lines.join("\n")
    } else {
        let mut new_frontmatter = vec![
            "---".to_string(),
            tag_line.clone(),
            "---".to_string(),
            String::new(),
        ];
        new_frontmatter.extend(lines);
        new_frontmatter.join("\n")
    }
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult, Error> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["[".to_string()]),
                    resolve_provider: Some(false),
                    completion_item: None,
                    all_commit_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Updated CodeAction provider using the new type and required fields.
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                        resolve_provider: Some(false),
                        work_done_progress_options: Default::default(),
                    },
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
            // Assumes that parse_markdown_symbols returns DocumentSymbol values with the deprecated field set.
            let symbols = parse_markdown_symbols(text);
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        } else {
            Ok(None)
        }
    }

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
                    // Re-insert the deprecated field with a value of None.
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

        let prefix = &current_line[..(position.character as usize).min(current_line.len())];
        let query = if let Some(idx) = prefix.rfind("[[") {
            &prefix[idx + 2..]
        } else {
            ""
        };

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

        let items = if !query.is_empty() {
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(i64, CompletionItem)> = candidates
                .into_iter()
                .filter_map(|item| {
                    matcher
                        .fuzzy_match(&item.label, query)
                        .map(|score| (score, item))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored.into_iter().map(|(_, item)| item).collect()
        } else {
            candidates
        };

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>, Error> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let text = {
            let docs = self.docs.lock().unwrap();
            docs.get(&uri).cloned()
        };

        let text = match text {
            Some(t) => t,
            None => return Ok(None),
        };

        let lines: Vec<&str> = text.lines().collect();
        if (position.line as usize) >= lines.len() {
            return Ok(None);
        }
        let line = lines[position.line as usize];

        let start_idx = line[..(position.character as usize)].rfind("[[");
        let end_idx = line[position.character as usize..].find("]]");

        if let (Some(start), Some(rel_end)) = (start_idx, end_idx) {
            let end = position.character as usize + rel_end;
            let wiki_text = &line[start + 2..end];
            let parts: Vec<&str> = wiki_text.split('|').collect();
            if parts.is_empty() {
                return Ok(None);
            }
            let vpath = parts[0].trim();
            let file_content =
                match utils::read_file(None, Some(vpath), true).map_err(|e| e.to_string()) {
                    Ok(c) => c,
                    Err(err) => {
                        self.client
                            .log_message(
                                MessageType::ERROR,
                                format!("Error reading file for hover: {}", err),
                            )
                            .await;
                        return Ok(None);
                    }
                };
            let preview: String = file_content.lines().collect::<Vec<&str>>().join("\n");

            let hover_value = format!("**Preview for {}**\n\n{}", vpath, preview);
            let hover_content = HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_value,
            });
            let hover = Hover {
                contents: hover_content,
                range: None,
            };
            Ok(Some(hover))
        } else {
            Ok(None)
        }
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>, Error> {
        let uri = params.text_document.uri;
        let text = {
            let docs = self.docs.lock().unwrap();
            docs.get(&uri).cloned()
        };

        let text = match text {
            Some(t) => t,
            None => return Ok(None),
        };

        // Clone text for use in the blocking task so the original remains available.
        let text_for_tags = text.clone();

        // Run generate_tags in a blocking task, converting errors to String.
        let tags_result = tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
            generate_tags(&text_for_tags).map_err(|e| e.to_string())
        })
        .await
        .map_err(|_| Error::internal_error())?;

        let tags = tags_result.map_err(|err_str| Error {
            code: tower_lsp::jsonrpc::ErrorCode::InternalError,
            message: Cow::Owned(err_str), // Convert String to Cow<'static, str>
            data: None,
        })?;

        let new_text = update_document_with_tags(&text, &tags);

        let lines: Vec<&str> = text.lines().collect();
        let end_position = if let Some(last_line) = lines.last() {
            Position {
                line: (lines.len() - 1) as u32,
                character: last_line.len() as u32,
            }
        } else {
            Position {
                line: 0,
                character: 0,
            }
        };
        let full_range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: end_position,
        };

        let text_edit = TextEdit {
            range: full_range,
            new_text,
        };

        let mut changes = std::collections::HashMap::new();
        changes.insert(uri.clone(), vec![text_edit]);
        let workspace_edit = WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        };

        let code_action = CodeAction {
            title: "Generate Tags".to_string(),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: None,
            edit: Some(workspace_edit),
            command: None,
            is_preferred: Some(true),
            disabled: None,
            data: None,
        };

        Ok(Some(vec![CodeActionOrCommand::CodeAction(code_action)]))
    }
}
