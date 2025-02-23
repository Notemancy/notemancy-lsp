use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::error::Error;
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::*;

pub struct MarkdownSymbolExtractor;

impl MarkdownSymbolExtractor {
    pub fn extract_symbols(
        content: &str,
        file_path: &str,
    ) -> Result<Vec<SymbolInformation>, Box<dyn Error>> {
        let mut symbols = Vec::new();
        let path = Path::new(file_path);

        // Convert to an absolute path (canonicalizing if possible)
        let abs_path: PathBuf = if path.is_absolute() {
            path.to_owned()
        } else {
            let joined = std::env::current_dir()?.join(path);
            if joined.exists() {
                joined.canonicalize()?
            } else {
                joined
            }
        };

        let file_url = Url::from_file_path(&abs_path)
            .map_err(|_| format!("Could not convert path {:?} to a file URL", abs_path))?;

        // Use the new parser with options and then get the offset iterator.
        let options = Options::empty(); // Enable extensions as needed.
        let parser = Parser::new_ext(content, options).into_offset_iter();

        let mut in_heading = false;
        let mut heading_text = String::new();
        let mut heading_start_offset: Option<usize> = None;
        let mut current_kind: Option<SymbolKind> = None;

        // Closure to compute line/character from a byte offset.
        let compute_position = |offset: usize| -> Position {
            let line = content[..offset].matches('\n').count() as u32;
            let last_newline = content[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let col = offset - last_newline;
            Position {
                line,
                character: col as u32,
            }
        };

        for (event, range) in parser {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    in_heading = true;
                    heading_text.clear();
                    current_kind = Some(match level {
                        HeadingLevel::H1 => SymbolKind::FILE,
                        HeadingLevel::H2 => SymbolKind::MODULE,
                        HeadingLevel::H3 => SymbolKind::NAMESPACE,
                        _ => SymbolKind::STRING,
                    });
                    heading_start_offset = None;
                }
                Event::Text(text) if in_heading => {
                    if heading_start_offset.is_none() {
                        heading_start_offset = Some(range.start);
                    }
                    heading_text.push_str(&text);
                }
                Event::End(TagEnd::Heading { .. }) if in_heading => {
                    let start_offset = heading_start_offset.unwrap_or(range.start);
                    let end_offset = range.end;
                    let start_position = compute_position(start_offset);
                    let end_position = compute_position(end_offset);

                    let symbol = SymbolInformation {
                        name: heading_text.clone(),
                        kind: current_kind.unwrap_or(SymbolKind::STRING),
                        location: Location {
                            uri: file_url.clone(),
                            range: Range {
                                start: start_position,
                                end: end_position,
                            },
                        },
                        container_name: None,
                        deprecated: None,
                        tags: None,
                    };
                    symbols.push(symbol);
                    in_heading = false;
                    current_kind = None;
                    heading_start_offset = None;
                }
                _ => {}
            }
        }

        Ok(symbols)
    }
}
