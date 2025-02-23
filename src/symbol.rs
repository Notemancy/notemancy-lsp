use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use tower_lsp::lsp_types::*;

#[derive(Debug, Clone)]
struct HeaderInfo {
    level: HeadingLevel,
    symbol: DocumentSymbol,
}

pub fn collect_symbols(parser: Parser) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    let mut current_line = 0;
    let mut header_stack: Vec<HeaderInfo> = Vec::new();
    let mut in_heading = false;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = true;
                // Pop headers that should be closed before starting this new header.
                while let Some(last) = header_stack.last() {
                    if last.level >= level {
                        let completed = header_stack.pop().unwrap();
                        if let Some(parent) = header_stack.last_mut() {
                            parent
                                .symbol
                                .children
                                .get_or_insert_with(Vec::new)
                                .push(completed.symbol);
                        } else {
                            symbols.push(completed.symbol);
                        }
                    } else {
                        break;
                    }
                }

                let new_header = HeaderInfo {
                    level,
                    symbol: DocumentSymbol {
                        name: String::new(),
                        detail: None,
                        kind: SymbolKind::STRING,
                        tags: None,
                        deprecated: Some(false),
                        range: Range {
                            start: Position {
                                line: current_line,
                                character: 0,
                            },
                            end: Position {
                                line: current_line,
                                character: 80,
                            },
                        },
                        selection_range: Range {
                            start: Position {
                                line: current_line,
                                character: 0,
                            },
                            end: Position {
                                line: current_line,
                                character: 80,
                            },
                        },
                        children: Some(Vec::new()),
                    },
                };
                header_stack.push(new_header);
            }
            Event::Text(text) if in_heading => {
                if let Some(header_info) = header_stack.last_mut() {
                    header_info.symbol.name.push_str(&text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                current_line += 1;
            }
            _ => {
                current_line += 1;
            }
        }
    }

    // Attach any remaining headers in the stack.
    while let Some(completed) = header_stack.pop() {
        if let Some(parent) = header_stack.last_mut() {
            parent
                .symbol
                .children
                .get_or_insert_with(Vec::new)
                .push(completed.symbol);
        } else {
            symbols.push(completed.symbol);
        }
    }

    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::Parser;

    #[test]
    fn test_single_heading() {
        let markdown = "# Header 1";
        let parser = Parser::new(markdown);
        let symbols = collect_symbols(parser);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Header 1");
        assert_eq!(symbols[0].children.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn test_multiple_headings_same_level() {
        let markdown = "# Header 1\n# Header 2\n# Header 3";
        let parser = Parser::new(markdown);
        let symbols = collect_symbols(parser);

        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].name, "Header 1");
        assert_eq!(symbols[1].name, "Header 2");
        assert_eq!(symbols[2].name, "Header 3");
    }

    #[test]
    fn test_nested_headings() {
        let markdown = "# Main\n## Sub 1\n### Deep 1\n## Sub 2";
        let parser = Parser::new(markdown);
        let symbols = collect_symbols(parser);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Main");

        let children = symbols[0].children.as_ref().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].name, "Sub 1");
        assert_eq!(children[1].name, "Sub 2");

        let sub_children = children[0].children.as_ref().unwrap();
        assert_eq!(sub_children.len(), 1);
        assert_eq!(sub_children[0].name, "Deep 1");
    }

    #[test]
    fn test_complex_hierarchy() {
        let markdown = "\
# Main
## Section 1
### Sub 1.1
### Sub 1.2
## Section 2
### Sub 2.1
#### Deep 2.1.1
### Sub 2.2";

        let parser = Parser::new(markdown);
        let symbols = collect_symbols(parser);

        assert_eq!(symbols.len(), 1);
        let main = &symbols[0];
        assert_eq!(main.name, "Main");

        let sections = main.children.as_ref().unwrap();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, "Section 1");
        assert_eq!(sections[1].name, "Section 2");

        let section1_subs = sections[0].children.as_ref().unwrap();
        assert_eq!(section1_subs.len(), 2);
        assert_eq!(section1_subs[0].name, "Sub 1.1");
        assert_eq!(section1_subs[1].name, "Sub 1.2");

        let section2_subs = sections[1].children.as_ref().unwrap();
        assert_eq!(section2_subs.len(), 2);
        assert_eq!(section2_subs[0].name, "Sub 2.1");
        assert_eq!(section2_subs[1].name, "Sub 2.2");

        let deep_sub = section2_subs[0].children.as_ref().unwrap();
        assert_eq!(deep_sub.len(), 1);
        assert_eq!(deep_sub[0].name, "Deep 2.1.1");
    }

    #[test]
    fn test_empty_document() {
        let markdown = "";
        let parser = Parser::new(markdown);
        let symbols = collect_symbols(parser);
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_document_no_headings() {
        let markdown = "Just some text\nwith multiple lines\nand no headings";
        let parser = Parser::new(markdown);
        let symbols = collect_symbols(parser);
        assert_eq!(symbols.len(), 0);
    }

    #[test]
    fn test_mixed_content() {
        let markdown = "\
# Header 1
Some text here
## Subheader
- List item
- Another item
### Deep header
```rust
let code = true;
```
## Another sub";

        let parser = Parser::new(markdown);
        let symbols = collect_symbols(parser);

        assert_eq!(symbols.len(), 1);
        let main = &symbols[0];
        assert_eq!(main.name, "Header 1");

        let subs = main.children.as_ref().unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].name, "Subheader");
        assert_eq!(subs[1].name, "Another sub");

        let deep = subs[0].children.as_ref().unwrap();
        assert_eq!(deep.len(), 1);
        assert_eq!(deep[0].name, "Deep header");
    }
}
