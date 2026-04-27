use crate::adapter::{self, LanguageAdapter};
use snif_types::*;
use tree_sitter::Query;

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn language_id(&self) -> Language {
        Language::Python
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn file_extensions(&self) -> &[&str] {
        &["py"]
    }

    fn import_query(&self) -> &str {
        r#"
        (import_statement name: (dotted_name) @module) @import
        (import_from_statement module_name: (dotted_name) @module) @from_import
        "#
    }

    fn symbol_query(&self) -> &str {
        r#"
        (function_definition name: (identifier) @name) @function
        (class_definition name: (identifier) @name) @class
        "#
    }

    fn reference_query(&self) -> &str {
        r#"
        (call
            function: [
                (identifier) @ref_name
                (attribute attribute: (identifier) @ref_name)
            ]
        ) @call
        "#
    }

    fn extract_imports(
        &self,
        source: &[u8],
        query: &Query,
        root: tree_sitter::Node,
    ) -> Vec<Import> {
        let matches = adapter::run_query_captures(query, root, source);
        let mut imports = Vec::new();

        for captures in &matches {
            let mut module = String::new();
            let mut line = 0;

            for (name, range, text) in captures {
                match name.as_str() {
                    "module" => module = text.clone(),
                    "import" | "from_import" => line = range.start_point.row + 1,
                    _ => {}
                }
            }

            if !module.is_empty() {
                imports.push(Import {
                    source: module,
                    names: vec![],
                    line,
                    alias: None,
                });
            }
        }

        imports
    }

    fn extract_symbols(
        &self,
        source: &[u8],
        query: &Query,
        root: tree_sitter::Node,
    ) -> Vec<Symbol> {
        let matches = adapter::run_query_captures(query, root, source);
        let mut symbols = Vec::new();

        for captures in &matches {
            let mut name = String::new();
            let mut kind = SymbolKind::Function;
            let mut start_line = 0;
            let mut end_line = 0;
            let mut body_text = String::new();

            for (cap_name, range, text) in captures {
                match cap_name.as_str() {
                    "name" => name = text.clone(),
                    "function" => {
                        kind = SymbolKind::Function;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    "class" => {
                        kind = SymbolKind::Class;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    _ => {}
                }
            }

            if !name.is_empty() {
                symbols.push(Symbol {
                    name,
                    kind,
                    start_line,
                    end_line,
                    signature: None,
                    body_text,
                    children: vec![],
                });
            }
        }

        symbols
    }
}
