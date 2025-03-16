use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notemancy_core::config; // Import the config module from notemancy-core crate
use notemancy_core::config::Config;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use url::Url;

use notemancy_core::db::crud;
use tower_lsp::lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse,
    InsertTextFormat,
};

#[derive(Clone, Debug)]
struct Backend {
    client: Client,
    /// A map from document URI to its full text.
    documents: Arc<Mutex<HashMap<Url, String>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(
        &self,
        _params: tower_lsp::lsp_types::InitializeParams,
    ) -> tower_lsp::jsonrpc::Result<tower_lsp::lsp_types::InitializeResult> {
        Ok(tower_lsp::lsp_types::InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                // Register the completion provider with trigger character "["
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec!["[".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: None,
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "notemancy-lsp initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents.lock().unwrap().insert(uri, text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next() {
            self.documents.lock().unwrap().insert(uri, change.text);
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let docs = self.documents.lock().unwrap();
        if let Some(text) = docs.get(&uri) {
            let symbols = parse_markdown_symbols(text);
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        } else {
            Ok(None)
        }
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> tower_lsp::jsonrpc::Result<Option<Vec<SymbolInformation>>> {
        let query = params.query;
        let inner_result = tokio::task::spawn_blocking(move || {
            // Read configuration and get the vault directory.
            let config = config::read_config().map_err(|e| e.to_string())?;
            let vault_dir = Path::new(&config.vault_dir);
            // Collect markdown files (deduplicated).
            let files = collect_markdown_files(vault_dir);
            let mut all_symbols = Vec::new();
            for file in files {
                let file_syms = extract_workspace_symbols_from_file(&file);
                all_symbols.extend(file_syms);
            }
            // Apply fuzzy filtering if a query is provided.
            let filtered = if query.trim().is_empty() {
                all_symbols
            } else {
                let mut matches: Vec<(usize, SymbolInformation)> = all_symbols
                    .into_iter()
                    .filter_map(|sym| fuzzy_match(&query, &sym.name).map(|score| (score, sym)))
                    .collect();
                matches.sort_by_key(|(score, _)| *score);
                matches.into_iter().map(|(_, sym)| sym).collect()
            };
            // Deduplicate symbols by using a key composed of (name, file URI, start line).
            let mut seen = HashSet::new();
            let deduped: Vec<_> = filtered
                .into_iter()
                .filter(|sym| {
                    let key = (
                        sym.name.clone(),
                        sym.location.uri.to_string(),
                        sym.location.range.start.line,
                    );
                    seen.insert(key)
                })
                .collect();
            Ok::<_, String>(deduped)
        })
        .await
        .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
        let symbols = inner_result.map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
        Ok(Some(symbols))
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> tower_lsp::jsonrpc::Result<Option<CompletionResponse>> {
        // Retrieve document URI and cursor position.
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let docs = self.documents.lock().unwrap();
        let text = if let Some(text) = docs.get(&uri) {
            text
        } else {
            return Ok(None);
        };

        // Check if the text up to the cursor ends with "[[".
        let lines: Vec<&str> = text.lines().collect();
        if position.line as usize >= lines.len() {
            return Ok(None);
        }
        let line = lines[position.line as usize];
        let col = position.character as usize;
        if col < 2 || !line[..col].ends_with("[[") {
            return Ok(None);
        }

        // Get the vault directory from the config.
        let config: Config = notemancy_core::config::read_config().map_err(|_e| {
            tower_lsp::jsonrpc::Error::new(tower_lsp::jsonrpc::ErrorCode::InternalError)
        })?;
        let vault_dir = std::path::Path::new(&config.vault_dir);

        // Query the database for pages (notes).
        let mut items = Vec::new();
        let db = notemancy_core::db::crud::global();
        if let Ok(mut stmt) = db.conn.prepare("SELECT vpath, title FROM pagetable") {
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            });
            if let Ok(rows) = rows {
                for row in rows.flatten() {
                    let (vpath, title) = row;
                    // Strip the vault dir from the vpath.
                    let relative_vpath = std::path::Path::new(&vpath)
                        .strip_prefix(vault_dir)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or(vpath.clone());
                    // Create a text edit that inserts our desired text at the current cursor position.
                    let text_edit = TextEdit {
                        range: Range {
                            start: position,
                            end: position,
                        },
                        new_text: format!("{} | {}", relative_vpath, title),
                    };
                    let item = CompletionItem {
                        label: title.clone(),
                        kind: Some(CompletionItemKind::FILE),
                        detail: Some(relative_vpath),
                        text_edit: Some(CompletionTextEdit::Edit(text_edit)),
                        ..Default::default()
                    };
                    items.push(item);
                }
            }
        }

        Ok(Some(CompletionResponse::Array(items)))
    }
}

/// Parses markdown text and extracts headings as document symbols.
fn parse_markdown_symbols(text: &str) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for (line_num, line) in text.lines().enumerate() {
        if let Some(stripped) = line.strip_prefix('#') {
            let mut level = 1;
            let mut rest = stripped;
            while rest.starts_with('#') {
                level += 1;
                rest = &rest[1..];
            }
            let title = rest.trim();
            if title.is_empty() {
                continue;
            }
            let start = Position {
                line: line_num as u32,
                character: 0,
            };
            let end = Position {
                line: line_num as u32,
                character: line.len() as u32,
            };
            let range = Range { start, end };

            symbols.push(DocumentSymbol {
                name: title.to_string(),
                detail: Some(format!("Heading level {}", level)),
                kind: SymbolKind::NAMESPACE,
                tags: None,
                range,
                selection_range: range,
                children: None,
                deprecated: None,
            });
        }
    }
    symbols
}

/// Reads a markdown file, extracts headings, and returns them as SymbolInformation.
fn extract_workspace_symbols_from_file(file_path: &Path) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();
    if let Ok(content) = fs::read_to_string(file_path) {
        let doc_symbols = parse_markdown_symbols(&content);
        if let Ok(uri) = Url::from_file_path(file_path) {
            for ds in doc_symbols {
                let sym_info = SymbolInformation {
                    name: ds.name,
                    kind: ds.kind,
                    location: Location {
                        uri: uri.clone(),
                        range: ds.range,
                    },
                    container_name: Some(
                        file_path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    deprecated: ds.deprecated,
                    tags: ds.tags,
                };
                symbols.push(sym_info);
            }
        }
    }
    symbols
}

/// A simple fuzzy matching function that returns a “gap” score if all query characters
/// are found in order within the candidate (ignoring case). Lower score indicates a better match.
fn fuzzy_match(query: &str, candidate: &str) -> Option<usize> {
    if query.trim().is_empty() {
        return Some(0);
    }
    let query = query.to_lowercase();
    let candidate = candidate.to_lowercase();
    let candidate_chars: Vec<char> = candidate.chars().collect();
    let mut pos_candidate = 0;
    let mut total_gap = 0;
    for qc in query.chars() {
        let mut found = false;
        while pos_candidate < candidate_chars.len() {
            if candidate_chars[pos_candidate] == qc {
                found = true;
                pos_candidate += 1;
                break;
            }
            total_gap += 1;
            pos_candidate += 1;
        }
        if !found {
            return None;
        }
    }
    Some(total_gap)
}

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::build(|client| Backend {
        client,
        documents: Arc::new(Mutex::new(HashMap::new())),
    })
    .finish();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use tower_lsp::lsp_types::Url;

    #[test]
    fn test_parse_markdown_symbols() {
        let text = r#"
# Heading1
Some text
## Heading2
More text
### Heading3
Even more text
Not a heading
"#;
        let symbols = parse_markdown_symbols(text);
        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].name, "Heading1");
        assert_eq!(symbols[1].name, "Heading2");
        assert_eq!(symbols[2].name, "Heading3");
    }

    #[tokio::test]
    async fn test_document_symbol() {
        let backend = {
            let mut backend_holder: Option<Backend> = None;
            let (_service, _socket) = LspService::build(|client| {
                let backend = Backend {
                    client,
                    documents: Arc::new(Mutex::new(HashMap::new())),
                };
                backend_holder = Some(backend.clone());
                backend
            })
            .finish();
            backend_holder.expect("Backend was not captured")
        };

        let uri = Url::parse("file:///test.md").unwrap();
        let content = "# Heading1\nSome text\n## Heading2".to_string();
        backend
            .did_open(DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri: uri.clone(),
                    language_id: "markdown".to_string(),
                    version: 1,
                    text: content,
                },
            })
            .await;

        let doc_symbols = backend
            .document_symbol(DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
            .unwrap();

        if let Some(DocumentSymbolResponse::Nested(symbols)) = doc_symbols {
            assert_eq!(symbols.len(), 2);
            assert_eq!(symbols[0].name, "Heading1");
            assert_eq!(symbols[1].name, "Heading2");
        } else {
            panic!("Expected nested document symbols");
        }
    }

    #[tokio::test]
    async fn test_workspace_symbol() {
        let backend = {
            let mut backend_holder: Option<Backend> = None;
            let (_service, _socket) = LspService::build(|client| {
                let backend = Backend {
                    client,
                    documents: Arc::new(Mutex::new(HashMap::new())),
                };
                backend_holder = Some(backend.clone());
                backend
            })
            .finish();
            backend_holder.expect("Backend was not captured")
        };

        let params = WorkspaceSymbolParams {
            query: "Head".to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        let response = backend.symbol(params).await.unwrap();
        assert!(response.is_some());
    }
}

/// Recursively collects markdown files from `dir`, deduplicating based on their canonical path.
fn collect_markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_markdown_files(&path));
            } else if let Some(ext) = path.extension() {
                if ext.to_string_lossy().eq_ignore_ascii_case("md") {
                    // Canonicalize to resolve symlinks.
                    if let Ok(canonical) = fs::canonicalize(&path) {
                        if seen.insert(canonical) {
                            files.push(path);
                        }
                    } else {
                        files.push(path);
                    }
                }
            }
        }
    }
    files
}
