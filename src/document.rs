// src/document.rs
use crate::symbol::collect_symbols;
use pulldown_cmark::Parser;
use tower_lsp::lsp_types::*;

pub struct Document {
    uri: Url,
    content: String,
}

impl Document {
    pub fn new(uri: Url, content: String) -> Self {
        Document { uri, content }
    }

    pub fn update_content(&mut self, content: String) {
        self.content = content;
    }

    pub fn get_symbols(&self) -> Vec<DocumentSymbol> {
        let parser = Parser::new(&self.content);
        collect_symbols(parser)
    }
}
