// src/handlers/workspace_symbol.rs
use crate::server::MarkdownLanguageServer;
use crate::symbol::markdown::MarkdownSymbolExtractor;
use std::fs;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn handle(
    server: &MarkdownLanguageServer,
    params: WorkspaceSymbolParams,
) -> Result<Option<Vec<SymbolInformation>>> {
    let query = params.query.to_lowercase();
    let mut symbols = Vec::new();

    // Get all files from the database
    if let Ok(files) = server.db.get_file_tree() {
        for file in files {
            // Only process markdown files
            if !file.path.ends_with(".md") {
                continue;
            }

            // Read file content
            if let Ok(content) = fs::read_to_string(&file.path) {
                if let Ok(file_symbols) =
                    MarkdownSymbolExtractor::extract_symbols(&content, &file.path)
                {
                    // Filter symbols based on query
                    for symbol in file_symbols {
                        if symbol.name.to_lowercase().contains(&query) {
                            symbols.push(symbol);
                        }
                    }
                }
            }
        }
    }

    Ok(Some(symbols))
}

// src/handlers/workspace_symbol.rs
#[cfg(test)]
mod tests {
    use crate::symbol::markdown::MarkdownSymbolExtractor;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_symbol_extraction() {
        // Create a temporary directory
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a test markdown file
        let content = "# Title\n## Section 1\n### Subsection";
        fs::write(&file_path, content).unwrap();

        // Test symbol extraction
        let results =
            MarkdownSymbolExtractor::extract_symbols(content, file_path.to_str().unwrap()).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "Title");
        assert_eq!(results[1].name, "Section 1");
        assert_eq!(results[2].name, "Subsection");
    }
}
