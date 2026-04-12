use std::collections::HashMap;

pub fn summarize_metrics(data: &[f64]) -> HashMap<String, f64> {
    let mut results = HashMap::new();
    let sum: f64 = data.iter().sum();
    results.insert("sum".to_string(), sum);
    results.insert("count".to_string(), data.len() as f64);
    results
}

/// Validates that all values in the dataset are within the given range.
pub fn validate_range(data: &[f64], min: f64, max: f64) -> bool {
    data.iter().all(|&v| v >= min && v <= max)
}
