/// Conservative token estimate for code with formatting.
/// Uses ~3 chars per token (overestimates slightly for safety).
/// Code has shorter tokens than prose due to operators, brackets, and short identifiers.
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 3
}
