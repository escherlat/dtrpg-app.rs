//! Deciding which side's tags win for one file.
//!
//! Three sets of tags are compared: what the file has, what the index has, and
//! what the two last agreed on (the snapshot). Whichever side differs from the
//! snapshot is the one that changed. If both changed, the later change wins;
//! if that cannot be told, the index wins (ADR 0008).

/// What to do with one file's tags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Decision {
    /// The file and the index agree, and the snapshot says so.
    InSync,
    /// The file and the index agree but the snapshot does not say so yet.
    Agreed,
    /// Make the file's tags the index's.
    WriteFile,
    /// Make the index's tags the file's.
    ReadFile,
}

/// The tags of one file as the three sides see them, and when each side last
/// changed.
#[derive(Clone, Copy, Debug)]
pub(super) struct Sides<'tags> {
    /// The tags the file and the index last agreed on, or `None` if they never
    /// have, which is the same as having agreed on no tags.
    pub(super) snapshot:         Option<&'tags [String]>,
    /// The tags the file has now.
    pub(super) file:             &'tags [String],
    /// The tags the index has now.
    pub(super) index:            &'tags [String],
    /// When the file's tags last changed, if the platform can say.
    pub(super) file_changed_at:  Option<i64>,
    /// When the index's tags last changed, if they ever have.
    pub(super) index_changed_at: Option<i64>,
}

/// Returns `tags` lower-cased, sorted and without repeats, to compare two lists
/// that are the same set however they are spelled or ordered.
pub(super) fn comparable(tags: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = tags.iter().map(|tag| tag.to_lowercase()).collect();
    tags.sort();
    tags.dedup();
    tags
}

/// Decides what to do with a file's tags.
pub(super) fn decide(sides: Sides<'_>) -> Decision {
    let file = comparable(sides.file);
    let index = comparable(sides.index);
    let base = sides.snapshot.map(comparable);
    if file == index {
        // Nothing to say about two files that have no tags and never had.
        return if base.as_ref() == Some(&file) || (base.is_none() && file.is_empty()) {
            Decision::InSync
        }
        else {
            Decision::Agreed
        };
    }
    let base = base.unwrap_or_default();
    match (file != base, index != base) {
        (false, _) => Decision::WriteFile,
        (true, false) => Decision::ReadFile,
        (true, true) => match (sides.file_changed_at, sides.index_changed_at) {
            (Some(file), Some(index)) if file > index => Decision::ReadFile,
            _ => Decision::WriteFile,
        },
    }
}

#[cfg(test)]
mod tests;
