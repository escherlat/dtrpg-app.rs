//! Timestamp conversion helpers shared by the SDK mapping layer and the UI's
//! own relative/absolute display formatting (`dtrpg_ui::util::datetime`).

use chrono::{DateTime, Utc};

/// Converts a Unix timestamp (seconds) to a UTC `DateTime`, falling back to
/// the earliest representable instant if `ts` is out of chrono's supported
/// range.
#[must_use]
pub fn from_epoch(ts: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(ts, 0).unwrap_or(DateTime::<Utc>::MIN_UTC)
}

/// Returns the current Unix timestamp in seconds.
#[must_use]
pub fn now_secs() -> i64 {
    Utc::now().timestamp()
}

/// Parses an RFC 3339 / ISO 8601 timestamp (e.g. `2024-07-16T10:45:52-05:00`
/// or `2024-07-16T10:45:52Z`) into a Unix timestamp in seconds.
///
/// # Examples
///
/// ```
/// let ts = dtrpg_core::util::datetime::parse_rfc3339_to_epoch("2024-07-16T10:45:52-05:00");
/// assert_eq!(ts, Some(1_721_144_752));
/// ```
pub fn parse_rfc3339_to_epoch(input: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(input).ok()
                                       .map(|dt| dt.timestamp())
}

/// Formats a Unix timestamp as an RFC 3339 / ISO 8601 string in UTC (e.g.
/// `2024-07-16T15:45:52+00:00`), the inverse of [`parse_rfc3339_to_epoch`].
///
/// Returns `None` if `epoch` is out of chrono's representable timestamp
/// range.
///
/// # Examples
///
/// ```
/// let iso = dtrpg_core::util::datetime::epoch_to_rfc3339(1_721_144_752);
/// assert_eq!(iso, Some("2024-07-16T15:45:52+00:00".to_string()));
/// ```
#[must_use]
pub fn epoch_to_rfc3339(epoch: i64) -> Option<String> {
    DateTime::from_timestamp(epoch, 0).map(|dt| dt.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_with_negative_offset() {
        // 2024-07-16T10:45:52-05:00 == 2024-07-16T15:45:52Z
        assert_eq!(parse_rfc3339_to_epoch("2024-07-16T10:45:52-05:00"),
                   Some(1_721_144_752));
    }

    #[test]
    fn parses_rfc3339_with_positive_offset() {
        // 2024-07-17T00:45:52+09:00 == 2024-07-16T15:45:52Z
        assert_eq!(parse_rfc3339_to_epoch("2024-07-17T00:45:52+09:00"),
                   Some(1_721_144_752));
    }

    #[test]
    fn parses_rfc3339_with_z_suffix() {
        assert_eq!(parse_rfc3339_to_epoch("2024-01-05T15:42:00Z"),
                   Some(1_704_469_320));
    }

    #[test]
    fn parses_rfc3339_epoch_zero() {
        assert_eq!(parse_rfc3339_to_epoch("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn rejects_malformed_rfc3339() {
        assert_eq!(parse_rfc3339_to_epoch("not-a-date"), None);
        assert_eq!(parse_rfc3339_to_epoch("2024-13-05T15:42:00Z"), None);
        assert_eq!(parse_rfc3339_to_epoch(""), None);
    }
}
