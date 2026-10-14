//! Formatting helpers for timestamps in the detail panel.

use chrono::{DateTime, Datelike, Utc};
use rust_i18n::t;

/// Converts a Unix timestamp (seconds) to a UTC `DateTime`, falling back to
/// the earliest representable instant if `ts` is out of chrono's supported
/// range.
fn from_epoch(ts: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(ts, 0).unwrap_or(DateTime::<Utc>::MIN_UTC)
}

/// Returns the current Unix timestamp in seconds.
#[must_use]
pub fn now_secs() -> i64 {
    Utc::now().timestamp()
}

/// Returns the whole number of calendar months between `then` (earlier) and
/// `now` (later), based on year/month/day components rather than a fixed
/// day-count division — so "5 months ago" lines up with the calendar month
/// a user would expect, not `elapsed / (30 * 86_400)`.
fn months_between(then: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    let mut months = (i64::from(now.year()) - i64::from(then.year())) * 12
                     + (i64::from(now.month()) - i64::from(then.month()));
    if now.day() < then.day() {
        months -= 1;
    }
    months.max(0)
}

/// Formats a Unix timestamp as a human-readable relative string.
///
/// Buckets (singular phrasing at N=1, e.g. "a minute ago" / "an hour ago",
/// numeral "N ..." otherwise):
/// - < 60 s      → "just now"
/// - < 60 m      → "a minute ago" / "N minutes ago"
/// - < 24 h      → "an hour ago" / "N hours ago"
/// - 24–47 h     → "yesterday"
/// - 2–6 days    → "N days ago"
/// - 7–29 days   → "a week ago" / "N weeks ago"
/// - 1–11 months → "a month ago" / "N months ago"
/// - 12+ months  → "a year ago" / "N years ago"
///
/// # Examples
///
/// ```
/// // Returns a relative string for a timestamp 2 hours ago.
/// let two_hours_ago = chrono::Utc::now().timestamp() - 7200;
/// let label = dtrpg_ui::util::datetime::format_relative(two_hours_ago);
/// assert!(label.contains("hours ago") || label == "yesterday");
/// ```
pub fn format_relative(ts: i64) -> String {
    let now = Utc::now();
    let then = from_epoch(ts);
    let elapsed = (now - then).num_seconds().max(0);

    if elapsed < 60 {
        return t!("date.now").to_string();
    }
    if elapsed < 3_600 {
        let mins = elapsed / 60;
        return match mins {
            1 => t!("date.minutes_ago.one").to_string(),
            _ => t!("date.minutes_ago.many", n = mins).to_string(),
        };
    }
    if elapsed < 86_400 {
        let hrs = elapsed / 3_600;
        return match hrs {
            1 => t!("date.hours_ago.one").to_string(),
            _ => t!("date.hours_ago.many", n = hrs).to_string(),
        };
    }
    if elapsed < 2 * 86_400 {
        return t!("date.yesterday").to_string();
    }
    if elapsed < 7 * 86_400 {
        let days = elapsed / 86_400;
        return t!("date.days_ago.many", n = days).to_string();
    }
    if elapsed < 30 * 86_400 {
        let weeks = elapsed / (7 * 86_400);
        return match weeks {
            1 => t!("date.weeks_ago.one").to_string(),
            _ => t!("date.weeks_ago.many", n = weeks).to_string(),
        };
    }

    // Guaranteed >= 1 by the 30-day threshold above in all but rare
    // short-month edge cases (e.g. 30 days from Jan 31 can still land in
    // February); floor at 1 so we never fall through to "0 months ago".
    let months = months_between(then, now).max(1);
    if months < 12 {
        return match months {
            1 => t!("date.months_ago.one").to_string(),
            _ => t!("date.months_ago.many", n = months).to_string(),
        };
    }

    let years = months / 12;
    match years {
        1 => t!("date.years_ago.one").to_string(),
        _ => t!("date.years_ago.many", n = years).to_string(),
    }
}

/// Parses an RFC 3339 / ISO 8601 timestamp (e.g. `2024-07-16T10:45:52-05:00`
/// or `2024-07-16T10:45:52Z`) into a Unix timestamp in seconds.
///
/// # Examples
///
/// ```
/// let ts = dtrpg_ui::util::datetime::parse_rfc3339_to_epoch("2024-07-16T10:45:52-05:00");
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
/// let iso = dtrpg_ui::util::datetime::epoch_to_rfc3339(1_721_144_752);
/// assert_eq!(iso, Some("2024-07-16T15:45:52+00:00".to_string()));
/// ```
#[must_use]
pub fn epoch_to_rfc3339(epoch: i64) -> Option<String> {
    DateTime::from_timestamp(epoch, 0).map(|dt| dt.to_rfc3339())
}

/// Formats `ts` as an absolute date/time using the OS's locale-preferred
/// date/time style (date field order, separators, and 12-/24-hour clock)
/// via `NSDateFormatter`.
///
/// Deliberately keeps the *timezone* fixed at UTC — only the *style* should
/// follow the OS locale, matching [`format_absolute`]'s existing
/// timezone-naive behavior; changing the displayed timezone as well is a
/// separate concern not requested here.
///
/// Returns `None` if formatting fails for any reason (should not happen in
/// practice); callers fall back to [`format_absolute`]'s fixed format.
#[cfg(target_os = "macos")]
fn format_absolute_os_locale(ts: i64) -> Option<String> {
    use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle, NSLocale, NSTimeZone};

    let formatter = NSDateFormatter::new();
    formatter.setDateStyle(NSDateFormatterStyle::MediumStyle);
    formatter.setTimeStyle(NSDateFormatterStyle::ShortStyle);
    formatter.setLocale(Some(&NSLocale::autoupdatingCurrentLocale()));
    formatter.setTimeZone(Some(&NSTimeZone::timeZoneForSecondsFromGMT(0)));
    let date = NSDate::dateWithTimeIntervalSince1970(ts as f64);
    Some(formatter.stringFromDate(&date).to_string())
}

/// Fixed-format absolute date/time string: "Month D, YYYY at H:MM AM/PM"
/// (e.g. "January 5, 2024 at 3:42 PM"), independent of OS locale.
///
/// Used by [`format_absolute`] on non-macOS platforms and as a fallback if
/// the OS-locale formatter fails; also what the unit tests below assert
/// against, since [`format_absolute`]'s OS-locale output is inherently
/// environment-dependent and can't be pinned to a literal string.
fn format_absolute_fixed(ts: i64) -> String {
    from_epoch(ts).format("%B %-d, %Y at %-I:%M %p").to_string()
}

/// Formats a Unix timestamp as a full absolute date/time string.
///
/// On macOS, the date field order, separators, and 12-/24-hour clock follow
/// the user's OS locale/region settings (via `NSDateFormatter`). Elsewhere
/// (and as a fallback if the OS-locale formatter fails), uses
/// [`format_absolute_fixed`]'s fixed format.
///
/// # Examples
///
/// ```
/// // Exact output varies by OS locale on macOS (date order, separators,
/// // 12-/24-hour clock), so this only checks that formatting succeeds.
/// let label = dtrpg_ui::util::datetime::format_absolute(0);
/// assert!(!label.is_empty());
/// ```
pub fn format_absolute(ts: i64) -> String {
    #[cfg(target_os = "macos")]
    if let Some(localized) = format_absolute_os_locale(ts) {
        return localized;
    }
    format_absolute_fixed(ts)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn now() -> i64 {
        Utc::now().timestamp()
    }

    #[test]
    fn just_now_at_59s() {
        assert_eq!(format_relative(now() - 59), "just now");
    }

    #[test]
    fn minutes_at_60s() {
        assert_eq!(format_relative(now() - 60), "a minute ago");
    }

    #[test]
    fn minutes_at_59m() {
        assert_eq!(format_relative(now() - 59 * 60), "59 minutes ago");
    }

    #[test]
    fn hours_at_1h() {
        assert_eq!(format_relative(now() - 3_600), "an hour ago");
    }

    #[test]
    fn hours_at_23h() {
        assert_eq!(format_relative(now() - 23 * 3_600), "23 hours ago");
    }

    #[test]
    fn yesterday_at_24h() {
        assert_eq!(format_relative(now() - 24 * 3_600), "yesterday");
    }

    #[test]
    fn yesterday_at_47h() {
        assert_eq!(format_relative(now() - 47 * 3_600), "yesterday");
    }

    #[test]
    fn days_at_48h() {
        assert_eq!(format_relative(now() - 48 * 3_600), "2 days ago");
    }

    #[test]
    fn days_at_6d() {
        assert_eq!(format_relative(now() - 6 * 86_400), "6 days ago");
    }

    #[test]
    fn weeks_at_7d() {
        assert_eq!(format_relative(now() - 7 * 86_400), "a week ago");
    }

    #[test]
    fn weeks_at_29d() {
        assert_eq!(format_relative(now() - 29 * 86_400), "4 weeks ago");
    }

    #[test]
    fn months_at_1_month() {
        let ts = months_ago_anchored(1);
        assert_eq!(format_relative(ts), "a month ago");
    }

    #[test]
    fn months_at_roughly_5_months() {
        // 5 calendar months ago, anchored on day-of-month to avoid drifting
        // across a short month (e.g. February).
        let ts = months_ago_anchored(5);
        assert_eq!(format_relative(ts), "5 months ago");
    }

    #[test]
    fn months_at_11_months() {
        let ts = months_ago_anchored(11);
        assert_eq!(format_relative(ts), "11 months ago");
    }

    #[test]
    fn years_at_12_months() {
        let ts = months_ago_anchored(12);
        assert_eq!(format_relative(ts), "a year ago");
    }

    #[test]
    fn years_at_25_months() {
        let ts = months_ago_anchored(25);
        assert_eq!(format_relative(ts), "2 years ago");
    }

    /// Returns a timestamp `n` whole calendar months before now, anchored to
    /// the first of the target month at noon so the subtraction never
    /// overflows into a different month than intended (e.g. subtracting from
    /// the 31st into a 30-day month).
    fn months_ago_anchored(n: u32) -> i64 {
        let now = Utc::now();
        let total = i64::from(now.year()) * 12 + i64::from(now.month()) - 1 - i64::from(n);
        let target_year = total.div_euclid(12) as i32;
        let target_month = total.rem_euclid(12) as u32 + 1;
        Utc.with_ymd_and_hms(target_year, target_month, 1, 12, 0, 0)
           .single()
           .map_or(0, |dt| dt.timestamp())
    }

    #[test]
    fn absolute_epoch_zero() {
        // Tests the deterministic fixed-format path directly —
        // `format_absolute` itself is OS-locale-dependent on macOS and
        // can't be pinned to a literal expected string (see
        // `format_absolute_os_locale`).
        assert_eq!(format_absolute_fixed(0), "January 1, 1970 at 12:00 AM");
    }

    #[test]
    fn absolute_known_date() {
        // 2024-01-05 15:42:00 UTC = 1704469320
        assert_eq!(format_absolute_fixed(1_704_469_320),
                   "January 5, 2024 at 3:42 PM");
    }

    #[test]
    fn absolute_is_never_empty() {
        assert!(!format_absolute(0).is_empty());
        assert!(!format_absolute(1_704_469_320).is_empty());
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
