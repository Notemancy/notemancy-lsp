use tower_lsp::lsp_types::*;

/// Parses Markdown text to extract headings (lines starting with `#`)
/// and returns document symbols.
pub fn parse_markdown_symbols(text: &str) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for (i, line) in text.lines().enumerate() {
        // Count consecutive '#' characters at the beginning.
        let heading_chars = line.chars().take_while(|&c| c == '#').count();
        if heading_chars > 0 {
            // Remove the leading '#' characters and trim whitespace.
            let name = line[heading_chars..].trim().to_string();
            let symbol = DocumentSymbol {
                name,
                detail: Some(format!("Heading level {}", heading_chars)),
                kind: SymbolKind::NAMESPACE,
                range: Range {
                    start: Position {
                        line: i as u32,
                        character: 0,
                    },
                    end: Position {
                        line: i as u32,
                        character: line.len() as u32,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: i as u32,
                        character: 0,
                    },
                    end: Position {
                        line: i as u32,
                        character: line.len() as u32,
                    },
                },
                children: None,
                deprecated: None,
                tags: None,
            };
            symbols.push(symbol);
        }
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::SymbolKind;

    #[test]
    fn test_parse_markdown_symbols() {
        let markdown = "\
# Heading 1
Some text here
## Heading 2
More text";

        let symbols = parse_markdown_symbols(markdown);
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Heading 1");
        assert_eq!(symbols[0].detail.as_deref(), Some("Heading level 1"));
        assert_eq!(symbols[1].name, "Heading 2");
        assert_eq!(symbols[1].detail.as_deref(), Some("Heading level 2"));
        assert_eq!(symbols[0].kind, SymbolKind::NAMESPACE);
    }
}
