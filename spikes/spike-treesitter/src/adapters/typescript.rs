use crate::parser::{self, LanguageAdapter};
use snif_common::*;
use tree_sitter::Query;

pub struct TypeScriptAdapter {
    is_tsx: bool,
}

impl TypeScriptAdapter {
    pub fn new(is_tsx: bool) -> Self {
        Self { is_tsx }
    }
}

impl LanguageAdapter for TypeScriptAdapter {
    fn language_id(&self) -> Language {
        Language::TypeScript
    }

    fn ts_language(&self) -> tree_sitter::Language {
        if self.is_tsx {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        }
    }

    fn file_extensions(&self) -> &[&str] {
        if self.is_tsx {
            &["tsx"]
        } else {
            &["ts"]
        }
    }

    fn import_query(&self) -> &str {
        r#"
        (import_statement
            source: (string) @source
        ) @import
        "#
    }

    fn symbol_query(&self) -> &str {
        r#"
        (function_declaration
            name: (identifier) @name
        ) @function

        (class_declaration
            name: (type_identifier) @name
        ) @class

        (interface_declaration
            name: (type_identifier) @name
        ) @interface

        (type_alias_declaration
            name: (type_identifier) @name
        ) @type_alias

        (enum_declaration
            name: (identifier) @name
        ) @enum

        (lexical_declaration
            (variable_declarator
                name: (identifier) @arrow_name
                value: (arrow_function) @arrow_body
            )
        ) @arrow_fn
        "#
    }

    fn reference_query(&self) -> &str {
        r#"
        (call_expression
            function: [
                (identifier) @ref_name
                (member_expression property: (property_identifier) @ref_name)
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
        let matches = parser::run_query_captures(query, root, source);
        let mut imports = Vec::new();

        for captures in &matches {
            let mut source_path = String::new();
            let mut line = 0;

            for (name, range, text) in captures {
                match name.as_str() {
                    "source" => {
                        // Remove quotes from string literal
                        source_path = text.trim_matches(|c| c == '\'' || c == '"').to_string();
                    }
                    "import" => {
                        line = range.start_point.row + 1;
                    }
                    _ => {}
                }
            }

            if !source_path.is_empty() {
                imports.push(Import {
                    source: source_path,
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
        let matches = parser::run_query_captures(query, root, source);
        let mut symbols = Vec::new();

        for captures in &matches {
            let mut name = String::new();
            let mut kind = SymbolKind::Function;
            let mut start_line = 0;
            let mut end_line = 0;
            let mut body_text = String::new();

            for (cap_name, range, text) in captures {
                match cap_name.as_str() {
                    "name" | "arrow_name" => {
                        name = text.clone();
                    }
                    "function" | "arrow_fn" => {
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
                    "interface" => {
                        kind = SymbolKind::Interface;
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
                    "enum" => {
                        kind = SymbolKind::Enum;
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
        let matches = parser::run_query_captures(query, root, source);
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
