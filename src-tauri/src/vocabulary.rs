use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const MAX_ENTRIES: usize = 100;
const MAX_TERM_CHARS: usize = 80;
const MAX_TERM_BYTES: usize = 240;
const MAX_ALIASES: usize = 10;
const MAX_PROMPT_BYTES: usize = 800;

fn default_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct VocabularyEntry {
    pub term: String,
    #[serde(default)]
    pub mishearings: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Validate user-supplied vocabulary before saving it.
pub fn validate(entries: &[VocabularyEntry]) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES {
        return Err(format!(
            "Vocabulary can contain at most {MAX_ENTRIES} entries"
        ));
    }

    let mut targets: HashMap<String, String> = HashMap::new();
    for (entry_index, entry) in entries.iter().enumerate() {
        validate_value(&entry.term, &format!("Entry {} term", entry_index + 1))?;
        if entry.mishearings.len() > MAX_ALIASES {
            return Err(format!(
                "Entry {} can contain at most {MAX_ALIASES} mishearings",
                entry_index + 1
            ));
        }

        let canonical_key = entry.term.to_lowercase();
        if entry.enabled {
            register_target(&mut targets, canonical_key, &entry.term)?;
        }
        for alias in &entry.mishearings {
            validate_value(alias, &format!("Entry {} mishearing", entry_index + 1))?;
            if entry.enabled {
                register_target(&mut targets, alias.to_lowercase(), &entry.term)?;
            }
        }
    }
    Ok(())
}

fn validate_value(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} cannot be empty"));
    }
    if value.chars().count() > MAX_TERM_CHARS || value.len() > MAX_TERM_BYTES {
        return Err(format!(
            "{label} must be at most {MAX_TERM_CHARS} characters and {MAX_TERM_BYTES} UTF-8 bytes"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{label} cannot contain control characters"));
    }
    Ok(())
}

fn register_target(
    targets: &mut HashMap<String, String>,
    key: String,
    canonical: &str,
) -> Result<(), String> {
    if let Some(existing) = targets.get(&key) {
        if existing != canonical {
            return Err("Vocabulary contains a case-insensitive spelling conflict".to_string());
        }
    } else {
        targets.insert(key, canonical.to_string());
    }
    Ok(())
}

/// Build a compact prompt hint from enabled preferred spellings only.
/// Entries are added whole and the result never exceeds 800 UTF-8 bytes.
pub fn hint_prompt(entries: &[VocabularyEntry]) -> String {
    let mut prompt = String::from("Preferred spellings: ");
    let mut seen = HashMap::<String, ()>::new();
    let mut added = false;

    for entry in entries.iter().filter(|entry| entry.enabled) {
        let key = entry.term.to_lowercase();
        if seen.contains_key(&key) {
            continue;
        }
        let separator = if added { ", " } else { "" };
        let additional_bytes = separator.len() + entry.term.len();
        if prompt.len() + additional_bytes > MAX_PROMPT_BYTES {
            break;
        }
        prompt.push_str(separator);
        prompt.push_str(&entry.term);
        seen.insert(key, ());
        added = true;
    }

    if added {
        prompt
    } else {
        String::new()
    }
}

/// Apply enabled aliases and canonical spellings against the original input.
/// Matches are case-insensitive, whole-word/phrase matches; longer overlapping
/// matches take precedence, and replacements are never reprocessed.
pub fn apply(text: &str, entries: &[VocabularyEntry]) -> String {
    let mut variants = Vec::<(String, String)>::new();
    for entry in entries.iter().filter(|entry| entry.enabled) {
        variants.push((entry.term.clone(), entry.term.clone()));
        variants.extend(
            entry
                .mishearings
                .iter()
                .map(|alias| (alias.clone(), entry.term.clone())),
        );
    }
    variants.sort_by(|(left, left_target), (right, right_target)| {
        right
            .chars()
            .count()
            .cmp(&left.chars().count())
            .then_with(|| left.to_lowercase().cmp(&right.to_lowercase()))
            .then_with(|| left_target.cmp(right_target))
    });

    let mut matches = Vec::<(usize, usize, String)>::new();
    for (variant, target) in variants {
        let Ok(regex) = Regex::new(&format!("(?i){}", regex::escape(&variant))) else {
            continue;
        };
        for found in regex.find_iter(text) {
            if has_boundaries(text, found.start(), found.end(), &variant) {
                matches.push((found.start(), found.end(), target.clone()));
            }
        }
    }

    matches.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| (b.1 - b.0).cmp(&(a.1 - a.0)))
            .then_with(|| a.2.cmp(&b.2))
    });

    let mut output = String::with_capacity(text.len());
    let mut copied_through = 0;
    for (start, end, replacement) in matches {
        if start < copied_through {
            continue;
        }
        output.push_str(&text[copied_through..start]);
        output.push_str(&replacement);
        copied_through = end;
    }
    output.push_str(&text[copied_through..]);
    output
}

fn has_boundaries(text: &str, start: usize, end: usize, variant: &str) -> bool {
    if variant.is_empty() {
        return false;
    }

    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();

    before.map_or(true, |ch| !is_word_char(ch)) && after.map_or(true, |ch| !is_word_char(ch))
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: &str, aliases: &[&str]) -> VocabularyEntry {
        VocabularyEntry {
            term: term.to_string(),
            mishearings: aliases.iter().map(|alias| alias.to_string()).collect(),
            enabled: true,
        }
    }

    #[test]
    fn corrects_case_insensitively_at_word_boundaries_and_repeats() {
        let entries = [entry("OpenAI", &["open eye"])];
        assert_eq!(
            apply("open eye, OPEN EYE! open eyes", &entries),
            "OpenAI, OpenAI! open eyes"
        );
    }

    #[test]
    fn supports_unicode_boundaries_and_punctuation_terms() {
        let entries = [
            entry("C++", &["see plus plus"]),
            entry("München", &["munich"]),
        ];
        assert_eq!(
            apply("see plus plus and MUNICH; C++ is useful", &entries),
            "C++ and München; C++ is useful"
        );
        assert_eq!(
            apply("preMünchen Münchenstraße", &entries),
            "preMünchen Münchenstraße"
        );
        assert_eq!(apply("C++x", &entries), "C++x");
    }

    #[test]
    fn disabled_entries_are_ignored() {
        let mut disabled = entry("Acme", &["ack me"]);
        disabled.enabled = false;
        assert_eq!(apply("ack me", &[disabled]), "ack me");
    }

    #[test]
    fn longest_phrase_wins_and_replacements_do_not_cascade() {
        let entries = [
            entry("New York City", &["new york"]),
            entry("York", &["big apple"]),
        ];
        assert_eq!(
            apply("new york then big apple", &entries),
            "New York City then York"
        );
        let cascading = [entry("B", &["A"]), entry("C", &["B extra"])];
        validate(&cascading).unwrap();
        assert_eq!(apply("A extra then B extra", &cascading), "B extra then C");
    }

    #[test]
    fn validation_rejects_ambiguous_case_insensitive_targets() {
        let entries = [entry("Acme", &["ACK ME"]), entry("Other", &["ack me"])];
        assert!(validate(&entries).is_err());
    }

    #[test]
    fn prompt_contains_only_whole_enabled_preferred_terms_and_is_bounded() {
        let mut entries = vec![entry(&"界".repeat(80), &[])];
        entries.push(entry("Disabled", &[]));
        entries[1].enabled = false;
        entries.extend((0..4).map(|index| entry(&format!("{}-{index}", "界".repeat(79)), &[])));
        let prompt = hint_prompt(&entries);
        assert!(prompt.len() <= MAX_PROMPT_BYTES);
        assert!(prompt.contains(&"界".repeat(80)));
        assert!(!prompt.contains("Disabled"));
        assert!(!prompt.contains("-3"));
    }
}
