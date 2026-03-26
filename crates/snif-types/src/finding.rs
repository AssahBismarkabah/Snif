use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub location: FileLocation,
    pub category: FindingCategory,
    pub confidence: f64,
    pub evidence: String,
    pub explanation: String,
    pub impact: String,
    pub suggestion: Option<String>,
    pub fingerprint: Option<Fingerprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLocation {
    #[serde(rename = "file")]
    pub path: String,
    pub start_line: usize,
    pub end_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingCategory {
    Logic,
    Security,
    Convention,
    Performance,
    Style,
    Other,
}

impl std::fmt::Display for FindingCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FindingCategory::Logic => write!(f, "logic"),
            FindingCategory::Security => write!(f, "security"),
            FindingCategory::Convention => write!(f, "convention"),
            FindingCategory::Performance => write!(f, "performance"),
            FindingCategory::Style => write!(f, "style"),
            FindingCategory::Other => write!(f, "other"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub id: String,
}
