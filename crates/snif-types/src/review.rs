use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetrievalMethod {
    Structural(StructuralReason),
    Semantic { distance: f64 },
    Keyword { matched_terms: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StructuralReason {
    DirectImport,
    ReverseImport,
    CoChange { correlation: f64 },
    SymbolReference { symbol_name: String },
}

#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub file_id: i64,
    pub path: String,
    pub score: f64,
    pub sources: Vec<RetrievalMethod>,
}

#[derive(Debug)]
pub struct RetrievalResults {
    pub results: Vec<RetrievalResult>,
    pub structural_count: usize,
    pub semantic_count: usize,
    pub keyword_count: usize,
}

#[derive(Debug)]
pub struct ContextPackage {
    pub metadata: ChangeMetadata,
    pub diff: String,
    pub changed_files: Vec<ContextFile>,
    pub related_files: Vec<ContextFile>,
    pub omissions: Vec<Omission>,
    pub budget: BudgetReport,
}

#[derive(Debug, Default)]
pub struct ChangeMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub base_branch: Option<String>,
    pub description: Option<String>,
    pub labels: Vec<String>,
    pub commit_messages: Vec<String>,
}

#[derive(Debug)]
pub struct ContextFile {
    pub path: String,
    pub content: String,
    pub summary: Option<String>,
    pub retrieval_score: Option<f64>,
}

#[derive(Debug)]
pub struct Omission {
    pub path: String,
    pub score: f64,
    pub reason: String,
}

#[derive(Debug)]
pub struct BudgetReport {
    pub total_budget: usize,
    pub diff_tokens: usize,
    pub changed_files_tokens: usize,
    pub related_files_tokens: usize,
    pub remaining_tokens: usize,
    pub files_included: usize,
    pub files_omitted: usize,
}
