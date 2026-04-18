/// Conservative token estimate for code with formatting.
/// Uses ~3 chars per token (overestimates slightly for safety).
/// Code has shorter tokens than prose due to operators, brackets, and short identifiers.
const TOKENS_PER_CHAR_RATIO: usize = 3;

pub fn estimate_tokens(text: &str) -> usize {
    text.len() / TOKENS_PER_CHAR_RATIO
}
