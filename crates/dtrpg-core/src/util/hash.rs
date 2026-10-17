//! Small, dependency-free hashing helpers shared across modules.

/// FNV-1a 32-bit hash of `s`.
///
/// Deterministic across runs and platforms — used wherever a stable,
/// filename-safe or bucket-safe digest of a string is needed (e.g. deriving a
/// cache filename from an item id, or picking a deterministic generative-cover
/// motif from a title).
///
/// # Examples
///
/// ```
/// use dtrpg_core::util::hash::fnv1a_32;
///
/// assert_eq!(fnv1a_32("same input"), fnv1a_32("same input"));
/// assert_ne!(fnv1a_32("a"), fnv1a_32("b"));
/// ```
#[must_use]
pub fn fnv1a_32(s: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for byte in s.bytes() {
        h ^= u32::from(byte);
        h = h.wrapping_mul(16_777_619);
    }
    h
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_is_deterministic() {
        assert_eq!(fnv1a_32("hello"), fnv1a_32("hello"));
    }

    #[test]
    fn different_input_usually_differs() {
        assert_ne!(fnv1a_32("hello"), fnv1a_32("world"));
    }

    #[test]
    fn empty_string_hashes_to_fnv_offset_basis() {
        assert_eq!(fnv1a_32(""), 2_166_136_261);
    }
}
