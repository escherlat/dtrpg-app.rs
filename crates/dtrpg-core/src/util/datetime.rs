//! Timestamp conversion helpers shared by the SDK mapping layer and the UI's
//! own relative/absolute display formatting (`dtrpg_ui::util::datetime`).

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};

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

/// Returns the Unix timestamp of midnight on the first day of the calendar
/// month containing `now`, in `now`'s own time zone.
///
/// Falls back to `now` itself in the unlikely case that local midnight does
/// not exist (a daylight-saving gap at the start of the month).
#[must_use]
pub fn start_of_month_secs(now: DateTime<Local>) -> i64 {
    Local.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
         .earliest()
         .map_or_else(|| now.timestamp(), |start| start.timestamp())
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
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn start_of_month_is_midnight_on_the_first_in_local_time() {
        let now = Local.with_ymd_and_hms(2026, 10, 19, 15, 30, 0).unwrap();

        let start = DateTime::from_timestamp(start_of_month_secs(now), 0).unwrap()
                                                                         .with_timezone(&Local);

        assert_eq!((start.year(), start.month(), start.day()), (2026, 10, 1));
        assert_eq!(start.format("%H:%M:%S").to_string(), "00:00:00");
    }

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
