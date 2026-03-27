pub fn collect_and_log(items: Vec<String>) -> (Vec<String>, String) {
    let mut collected = Vec::new();
    for item in items {
        collected.push(item.clone());
    }
    let summary = format!("Collected {} items, last: {}", collected.len(), collected.last().unwrap_or(&String::new()));
    (collected, summary)
}

pub fn process_batch(names: Vec<String>) -> Vec<String> {
    let moved = names;
    let count = moved.len();
    tracing::info!("Processing {} names", count);
    moved
}
