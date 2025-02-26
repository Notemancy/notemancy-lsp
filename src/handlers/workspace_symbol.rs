// src/handlers/workspace_symbol.rs
use crate::server::MarkdownLanguageServer;
use crate::symbol::markdown::MarkdownSymbolExtractor;
use rayon::prelude::*;
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
        // Use Rayon to process files in parallel
        let file_symbols: Vec<Vec<SymbolInformation>> = files
            .par_iter() // Convert the iterator to a parallel iterator
            .filter_map(|file| {
                // Only process markdown files
                if !file.path.ends_with(".md") {
                    return None;
                }

                // Read file content
                if let Ok(content) = fs::read_to_string(&file.path) {
                    // Extract symbols from the content
                    if let Ok(file_symbols) =
                        MarkdownSymbolExtractor::extract_symbols(&content, &file.path)
                    {
                        // Filter symbols based on query
                        let filtered_symbols: Vec<SymbolInformation> = file_symbols
                            .into_iter()
                            .filter(|symbol| symbol.name.to_lowercase().contains(&query))
                            .collect();

                        // Return the filtered symbols
                        Some(filtered_symbols)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Flatten the results from all the files into one vector
        symbols.extend(file_symbols.into_iter().flatten());
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
