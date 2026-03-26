use anyhow::{Context, Result};
use snif_types::{FileExtraction, Import, Language, ParseError, Reference, Symbol};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

pub trait LanguageAdapter: Send + Sync {
    fn language_id(&self) -> Language;
    fn ts_language(&self) -> tree_sitter::Language;
    fn file_extensions(&self) -> &[&str];
    fn import_query(&self) -> &str;
    fn symbol_query(&self) -> &str;
    fn reference_query(&self) -> &str;
    fn extract_imports(&self, source: &[u8], query: &Query, root: tree_sitter::Node) -> Vec<Import>;
    fn extract_symbols(&self, source: &[u8], query: &Query, root: tree_sitter::Node) -> Vec<Symbol>;
    fn extract_references(&self, source: &[u8], query: &Query, root: tree_sitter::Node) -> Vec<Reference>;
}

pub fn parse_file(adapter: &dyn LanguageAdapter, path: &str, source: &[u8]) -> Result<FileExtraction> {
    let mut parser = Parser::new();
    parser
        .set_language(&adapter.ts_language())
        .context("Failed to set parser language")?;

    let tree = parser
        .parse(source, None)
        .context("Failed to parse file")?;

    let root = tree.root_node();

    let parse_errors = collect_errors(root);

    let import_query = Query::new(&adapter.ts_language(), adapter.import_query())
        .context("Failed to compile import query")?;
    let imports = adapter.extract_imports(source, &import_query, root);

    let symbol_query = Query::new(&adapter.ts_language(), adapter.symbol_query())
        .context("Failed to compile symbol query")?;
    let symbols = adapter.extract_symbols(source, &symbol_query, root);

    let ref_query = Query::new(&adapter.ts_language(), adapter.reference_query())
        .context("Failed to compile reference query")?;
    let references = adapter.extract_references(source, &ref_query, root);

    Ok(FileExtraction {
        path: path.to_string(),
        language: adapter.language_id(),
        imports,
        symbols,
        references,
        parse_errors,
    })
}

fn collect_errors(root: tree_sitter::Node) -> Vec<ParseError> {
    let mut errors = Vec::new();
    let mut cursor = root.walk();
    collect_errors_recursive(&mut cursor, &mut errors);
    errors
}

fn collect_errors_recursive(cursor: &mut tree_sitter::TreeCursor, errors: &mut Vec<ParseError>) {
    let node = cursor.node();
    if node.is_error() || node.is_missing() {
        let start = node.start_position();
        errors.push(ParseError {
            line: start.row + 1,
            column: start.column,
            message: if node.is_missing() {
                format!("missing {}", node.kind())
            } else {
                "syntax error".to_string()
            },
        });
    }

    if cursor.goto_first_child() {
        loop {
            collect_errors_recursive(cursor, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

pub fn node_text<'a>(source: &'a [u8], node: tree_sitter::Node) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).unwrap_or("<invalid utf8>")
}

pub fn run_query_captures(
    query: &Query,
    root: tree_sitter::Node,
    source: &[u8],
) -> Vec<Vec<(String, tree_sitter::Range, String)>> {
    let mut cursor = QueryCursor::new();
    let capture_names: Vec<String> = query.capture_names().iter().map(|s| s.to_string()).collect();

    let mut results = Vec::new();
    let mut matches = cursor.matches(query, root, source);
    while let Some(m) = matches.next() {
        let captures: Vec<(String, tree_sitter::Range, String)> = m
            .captures
            .iter()
            .map(|c| {
                let name = capture_names[c.index as usize].clone();
                let range = c.node.range();
                let text = node_text(source, c.node).to_string();
                (name, range, text)
            })
            .collect();
        results.push(captures);
    }

    results
}
