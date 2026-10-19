//! The library search box: free text, and `tag:` terms.
//!
//! `tag:solo` keeps only the items that have the tag "solo", and `-tag:solo`
//! only those that do not. A name with spaces is quoted: `tag:"to run"`.
//! Anything else is free text, matched against an item's title, publisher, game
//! line and tag names. Tag names are compared ignoring case.

use std::sync::Arc;

use crate::data::library::LibraryItem;

/// A search as the user typed it, split into what it asks for.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchQuery {
    /// The free text, or empty. Lower cased.
    text:         String,
    /// The tags an item must have, lower cased.
    with_tags:    Vec<String>,
    /// The tags an item must not have, lower cased.
    without_tags: Vec<String>,
}

/// Removes `prefix` from the start of `text`, ignoring ASCII case.
fn strip_prefix_ignoring_case<'text>(text: &'text str, prefix: &str) -> Option<&'text str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// Reads a tag name from the start of `text`: a quoted name up to the closing
/// quote (or the end), or up to the next space. Returns the name and the rest.
fn read_tag_name(text: &str) -> (&str, &str) {
    if let Some(quoted) = text.strip_prefix('"') {
        return match quoted.split_once('"') {
            Some((name, rest)) => (name, rest),
            None => (quoted, ""),
        };
    }
    text.split_once(char::is_whitespace).unwrap_or((text, ""))
}

impl SearchQuery {
    /// Splits what the user typed into free text and tag terms.
    ///
    /// A query with no tag term keeps its text exactly as typed, so a plain
    /// search behaves as it always has. A `tag:` with no name is a tag term
    /// that asks for nothing, and is left out of the text.
    #[must_use]
    pub fn parse(query: &str) -> Self {
        let mut parsed = Self::default();
        let mut words: Vec<&str> = Vec::new();
        let mut saw_a_term = false;
        let mut rest = query.trim_start();
        while !rest.is_empty() {
            let term = strip_prefix_ignoring_case(rest, "-tag:").map(|after| (true, after))
                                                                .or_else(|| {
                                                                    strip_prefix_ignoring_case(rest, "tag:").map(|after| (false, after))
                                                                });
            if let Some((negated, after)) = term {
                saw_a_term = true;
                let (name, remainder) = read_tag_name(after);
                let name = name.trim().to_lowercase();
                if !name.is_empty() {
                    if negated {
                        parsed.without_tags.push(name);
                    }
                    else {
                        parsed.with_tags.push(name);
                    }
                }
                rest = remainder.trim_start();
            }
            else {
                let (word, remainder) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                words.push(word);
                rest = remainder.trim_start();
            }
        }
        parsed.text = if saw_a_term {
            words.join(" ").to_lowercase()
        }
        else {
            query.to_lowercase()
        };
        parsed
    }

    /// Whether the search has any tag term, and so needs each item's tags.
    #[must_use]
    pub fn has_tag_terms(&self) -> bool {
        !self.with_tags.is_empty() || !self.without_tags.is_empty()
    }

    /// Whether `item`, which has the tags `tags`, matches the search.
    #[must_use]
    pub fn matches(&self, item: &LibraryItem, tags: &[Arc<str>]) -> bool {
        let has = |wanted: &String| tags.iter().any(|tag| tag.to_lowercase() == *wanted);
        if !self.with_tags.iter().all(has) || self.without_tags.iter().any(has) {
            return false;
        }
        self.text.is_empty()
        || item.title.to_lowercase().contains(&self.text)
        || item.publisher.to_lowercase().contains(&self.text)
        || item.line.to_lowercase().contains(&self.text)
        || tags.iter()
               .any(|tag| tag.to_lowercase().contains(&self.text))
    }
}

#[cfg(test)]
mod tests;
