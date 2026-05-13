//! Sequencer — Shannon entropy and basic character-class analysis for
//! captured session tokens, CSRF tokens, etc. Useful for spotting
//! predictable identifiers.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencerReport {
    pub samples: usize,
    pub mean_length: f64,
    pub shannon_entropy_bits: f64,
    pub character_classes: HashMap<String, usize>,
    pub uniqueness_ratio: f64,
}

pub fn analyze<I, S>(samples: I) -> SequencerReport
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let samples_vec: Vec<String> = samples.into_iter().map(|s| s.as_ref().to_string()).collect();
    let n = samples_vec.len();

    if n == 0 {
        return SequencerReport {
            samples: 0,
            mean_length: 0.0,
            shannon_entropy_bits: 0.0,
            character_classes: HashMap::new(),
            uniqueness_ratio: 0.0,
        };
    }

    let mean_length =
        samples_vec.iter().map(|s| s.chars().count() as f64).sum::<f64>() / n as f64;

    let unique: std::collections::HashSet<&String> = samples_vec.iter().collect();
    let uniqueness_ratio = unique.len() as f64 / n as f64;

    let combined: String = samples_vec.join("");

    let mut freq: HashMap<char, usize> = HashMap::new();
    for c in combined.chars() {
        *freq.entry(c).or_insert(0) += 1;
    }
    let total = combined.chars().count() as f64;
    let shannon_entropy_bits = if total == 0.0 {
        0.0
    } else {
        freq.values()
            .map(|&count| {
                let p = count as f64 / total;
                -p * p.log2()
            })
            .sum()
    };

    let mut classes: HashMap<String, usize> = HashMap::new();
    for c in combined.chars() {
        let key = if c.is_ascii_uppercase() {
            "upper"
        } else if c.is_ascii_lowercase() {
            "lower"
        } else if c.is_ascii_digit() {
            "digit"
        } else if c.is_ascii_punctuation() {
            "punct"
        } else if c.is_ascii_whitespace() {
            "ws"
        } else {
            "other"
        };
        *classes.entry(key.into()).or_insert(0) += 1;
    }

    SequencerReport {
        samples: n,
        mean_length,
        shannon_entropy_bits,
        character_classes: classes,
        uniqueness_ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_zero_report() {
        let r = analyze::<Vec<&str>, &str>(vec![]);
        assert_eq!(r.samples, 0);
        assert_eq!(r.shannon_entropy_bits, 0.0);
    }

    #[test]
    fn high_entropy_for_random_strings() {
        let r = analyze([
            "kJ8hT3plQ1aF5z",
            "xY7uIo9pLk2mB3",
            "qW8eR1tY4uI6oP",
            "aS3dF6gH9jK2lM",
        ]);
        assert!(r.shannon_entropy_bits > 4.0);
        assert_eq!(r.samples, 4);
        assert!(r.uniqueness_ratio > 0.9);
    }

    #[test]
    fn low_entropy_for_repetitive_strings() {
        let r = analyze(["aaaaaa", "aaaaaa", "aaaaaa"]);
        assert!(r.shannon_entropy_bits < 1.0);
        assert!(r.uniqueness_ratio < 0.5);
    }

    #[test]
    fn character_classes_are_tracked() {
        let r = analyze(["Abc123"]);
        assert_eq!(r.character_classes.get("upper").copied(), Some(1));
        assert_eq!(r.character_classes.get("lower").copied(), Some(2));
        assert_eq!(r.character_classes.get("digit").copied(), Some(3));
    }
}
