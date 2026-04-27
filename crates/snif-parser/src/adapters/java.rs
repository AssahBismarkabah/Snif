use crate::adapter::{self, LanguageAdapter};
use snif_types::*;
use tree_sitter::Query;

pub struct JavaAdapter;

impl LanguageAdapter for JavaAdapter {
    fn language_id(&self) -> Language {
        Language::Java
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn file_extensions(&self) -> &[&str] {
        &["java"]
    }

    fn import_query(&self) -> &str {
        r#"
        (package_declaration (scoped_identifier) @package_name) @package
        (import_declaration
          [
            (scoped_identifier) @import_path
            (identifier) @import_path
          ]
        ) @import
        "#
    }

    fn symbol_query(&self) -> &str {
        r#"
        (class_declaration name: (identifier) @name) @class
        (interface_declaration name: (identifier) @name) @interface
        (enum_declaration name: (identifier) @name) @enum
        (record_declaration name: (identifier) @name) @record
        (method_declaration name: (identifier) @name) @method
        (constructor_declaration name: (identifier) @name) @constructor
        "#
    }

    fn reference_query(&self) -> &str {
        r#"
        (method_invocation name: (identifier) @ref_name) @call
        (object_creation_expression type: (type_identifier) @ref_name) @ctor_call
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
            let mut import_path = String::new();
            let mut line = 0;
            let mut is_import = false;

            for (name, range, text) in captures {
                match name.as_str() {
                    "import_path" => {
                        import_path = text.clone();
                    }
                    "import" => {
                        line = range.start_point.row + 1;
                        is_import = true;
                    }
                    "package_name" => {
                        import_path = text.clone();
                    }
                    "package" => {
                        line = range.start_point.row + 1;
                    }
                    _ => {}
                }
            }

            if !import_path.is_empty() && is_import {
                let (source_pkg, names) = parse_java_import(&import_path);
                imports.push(Import {
                    source: source_pkg,
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
            let mut kind = SymbolKind::Class;
            let mut start_line = 0;
            let mut end_line = 0;
            let mut body_text = String::new();

            for (cap_name, range, text) in captures {
                match cap_name.as_str() {
                    "name" => {
                        name = text.clone();
                    }
                    "class" | "record" => {
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
                    "enum" => {
                        kind = SymbolKind::Enum;
                        start_line = range.start_point.row + 1;
                        end_line = range.end_point.row + 1;
                        body_text = text.clone();
                    }
                    "method" | "constructor" => {
                        kind = SymbolKind::Method;
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

fn parse_java_import(path: &str) -> (String, Vec<String>) {
    let cleaned = path.trim();

    // Wildcard: "java.util.*" or just ends with asterisk in the import
    // The tree-sitter captures the scoped_identifier before the asterisk
    // so "java.util" is captured, not "java.util.*"

    if let Some(last_dot) = cleaned.rfind('.') {
        let package = cleaned[..last_dot].to_string();
        let name = cleaned[last_dot + 1..].to_string();
        if name == "*" {
            (package, vec![])
        } else {
            (package, vec![name])
        }
    } else {
        (cleaned.to_string(), vec![])
    }
}
