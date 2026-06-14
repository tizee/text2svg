// Dictionary-based hyphenation (Knuth-Liang) using embedded English patterns.
//
// Thin wrapper over the `hyphenation` crate. Produces character-index break
// points within a single word so callers can insert soft hyphens (U+00AD) and
// let the existing line-breaking pipeline pick break points.

use hyphenation::{Hyphenator as _, Language, Load, Standard};

/// A loaded hyphenation dictionary.
pub struct Hyphenator {
    dict: Standard,
}

impl Hyphenator {
    /// Load the embedded English (US) dictionary.
    pub fn english_us() -> Option<Self> {
        Standard::from_embedded(Language::EnglishUS)
            .ok()
            .map(|dict| Self { dict })
    }

    /// Return the character indices at which `word` may be hyphenated.
    ///
    /// Only alphabetic words of at least 4 characters are considered; the
    /// underlying dictionary additionally enforces a minimum number of
    /// characters on each side of a break. Returns an empty vector when the
    /// word should not be hyphenated.
    pub fn break_indices(&self, word: &str) -> Vec<usize> {
        if word.chars().count() < 4 || !word.chars().all(|c| c.is_alphabetic()) {
            return Vec::new();
        }
        self.dict.hyphenate(word).breaks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_embedded_english_dictionary() {
        assert!(
            Hyphenator::english_us().is_some(),
            "embedded en-us dictionary must load"
        );
    }

    #[test]
    fn hyphenates_a_multisyllable_word() {
        let h = Hyphenator::english_us().expect("dict");
        let breaks = h.break_indices("hyphenation");
        assert!(
            !breaks.is_empty(),
            "a long word must have at least one hyphenation point"
        );
        // All break points must be strictly inside the word.
        let len = "hyphenation".chars().count();
        for &b in &breaks {
            assert!(b > 0 && b < len, "break {} out of range for len {}", b, len);
        }
    }

    #[test]
    fn does_not_hyphenate_short_words() {
        let h = Hyphenator::english_us().expect("dict");
        assert!(h.break_indices("cat").is_empty(), "3-letter word");
        assert!(h.break_indices("to").is_empty(), "2-letter word");
    }

    #[test]
    fn does_not_hyphenate_non_alphabetic() {
        let h = Hyphenator::english_us().expect("dict");
        assert!(h.break_indices("12345").is_empty());
        assert!(h.break_indices("a1b2c3").is_empty());
    }
}
