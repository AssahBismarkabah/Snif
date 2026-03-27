use crate::adapter::{self, LanguageAdapter};
use snif_types::*;
use tree_sitter::Query;

pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn language_id(&self) -> Language {
        Language::Rust
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn file_extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn import_query(&self) -> &str {
        "(use_declaration argument: (_) @path) @import"
    }

    fn symbol_query(&self) -> &str {
        r#"
        (function_item name: (identifier) @name) @function
        (struct_item name: (type_identifier) @name) @struct
        (enum_item name: (type_identifier) @name) @enum
        (trait_item name: (type_identifier) @name) @trait
        (type_item name: (type_identifier) @name) @type_alias
        (mod_item name: (identifier) @mod_name) @module
        (const_item name: (identifier) @const_name) @constant
        (static_item name: (identifier) @static_name) @static_item
        "#
    }

    fn reference_query(&self) -> &str {
        r#"
        (call_expression
            function: [
                (identifier) @ref_name
                (field_expression field: (field_identifier) @ref_name)
                (scoped_identifier name: (identifier) @ref_name)
            ]) @call
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
            let mut path = String::new();
            let mut line = 0;

            for (name, range, text) in captures {
                match name.as_str() {
                    "path" => path = text.clone(),
                    "import" => line = range.start_point.row + 1,
                    _ => {}
                }
            }

            if !path.is_empty() {
                let (source_mod, names) = parse_use_path(&path);
                imports.push(Import {
                    source: source_mod,
                    names,
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
                    "name" | "mod_name" | "const_name" | "static_name" => {
                        name = text.clone();
                    }
                    "function" => {
                        kind = SymbolKind::Function;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    "struct" => {
                        kind = SymbolKind::Struct;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    "enum" => {
                        kind = SymbolKind::Enum;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    "trait" => {
                        kind = SymbolKind::Trait;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    "type_alias" => {
                        kind = SymbolKind::TypeAlias;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    "module" => {
                        kind = SymbolKind::Module;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    "constant" | "static_item" => {
                        kind = SymbolKind::Constant;
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

    fn extract_references(
        &self,
        source: &[u8],
        query: &Query,
        root: tree_sitter::Node,
    ) -> Vec<Reference> {
        let matches = adapter::run_query_captures(query, root, source);
        let mut refs = Vec::new();

        for captures in &matches {
            for (cap_name, range, text) in captures {
                if cap_name == "ref_name" {
                    refs.push(Reference {
                        name: text.clone(),
                        line: range.start_point.row + 1,
                        context: String::new(),
                    });
                }
            }
        }

        refs
    }
}

fn parse_use_path(path: &str) -> (String, Vec<String>) {
    let cleaned = path.trim();

    if let Some(brace_start) = cleaned.find('{') {
        let prefix = cleaned[..brace_start].trim_end_matches(':').to_string();
        let brace_content = &cleaned[brace_start + 1..cleaned.len() - 1];
        let names: Vec<String> = brace_content
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        (prefix, names)
    } else if cleaned.contains("::") {
        let parts: Vec<&str> = cleaned.rsplitn(2, "::").collect();
        if parts.len() == 2 {
            (parts[1].to_string(), vec![parts[0].to_string()])
        } else {
            (cleaned.to_string(), vec![])
        }
    } else {
        (cleaned.to_string(), vec![])
    }
}
