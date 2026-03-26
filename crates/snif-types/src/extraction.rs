use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    TypeScript,
    Python,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Language::Rust => write!(f, "rust"),
            Language::TypeScript => write!(f, "typescript"),
            Language::Python => write!(f, "python"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileExtraction {
    pub path: String,
    pub language: Language,
    pub imports: Vec<Import>,
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub parse_errors: Vec<ParseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub source: String,
    pub names: Vec<String>,
    pub line: usize,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    TypeAlias,
    Module,
    Constant,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Function => write!(f, "function"),
            SymbolKind::Method => write!(f, "method"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Class => write!(f, "class"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Interface => write!(f, "interface"),
            SymbolKind::TypeAlias => write!(f, "type_alias"),
            SymbolKind::Module => write!(f, "module"),
            SymbolKind::Constant => write!(f, "constant"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub body_text: String,
    pub children: Vec<Symbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub name: String,
    pub line: usize,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}
