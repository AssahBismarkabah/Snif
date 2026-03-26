pub fn calculateTotal(items: &[f64]) -> f64 {
    let mut total = 0.0;
    for item in items {
        total += item;
    }
    total
}

pub fn IS_VALID(input: &str) -> bool {
    !input.is_empty() && input.len() < 256
}

pub fn   format_output(   data: &str   ) -> String {
    format!("[OUTPUT] {}", data)
}
