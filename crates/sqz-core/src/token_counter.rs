use std::sync::OnceLock;

use tiktoken_rs::CoreBPE;

/// Global lazy-initialized tokenizer (cl100k_base is used by GPT-4 / ChatGPT).
/// Initialization can be slow (~100ms), so we do it once and share.
static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();

fn get_tokenizer() -> &'static CoreBPE {
    TOKENIZER.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("failed to initialize cl100k_base tokenizer")
    })
}

/// A lightweight wrapper around tiktoken-rs for counting tokens.
#[derive(Debug, Clone)]
pub struct TokenCounter;

impl TokenCounter {
    /// Create a new `TokenCounter`. The underlying tokenizer is lazily
    /// initialized on first use and shared across all instances.
    pub fn new() -> Self {
        TokenCounter
    }

    /// Count the number of tokens in the given text using cl100k_base encoding.
    pub fn count(&self, text: &str) -> usize {
        let bpe = get_tokenizer();
        bpe.encode_with_special_tokens(text).len()
    }
}

impl Default for TokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_count_basic() {
        let counter = TokenCounter::new();
        let count = counter.count("hello world");
        assert!(count > 0);
    }

    #[test]
    fn test_empty_string() {
        let counter = TokenCounter::new();
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn test_consistent_counts() {
        let counter = TokenCounter::new();
        let text = "The quick brown fox jumps over the lazy dog";
        let c1 = counter.count(text);
        let c2 = counter.count(text);
        assert_eq!(c1, c2);
    }
}
