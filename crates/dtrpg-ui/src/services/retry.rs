//! Shared retry-with-backoff helper for resource-fetch operations that fail
//! with a transient error: catalog synchronization, cover/avatar image
//! caching, and (once implemented) download transfers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How long to sleep between cancellation checks while waiting out a backoff
/// delay, mirroring `download.rs`'s per-chunk cancellation granularity.
const BACKOFF_TICK: Duration = Duration::from_millis(200);

/// Computes an exponential backoff delay with deterministic jitter.
///
/// `attempt` starts at `1` for the delay before the first retry. The delay is
/// `base_secs * 2^(attempt - 1)`, capped at `max_secs`, then jittered by up to
/// +/-25% deterministically from `jitter_source` (typically
/// `SystemTime::now()`'s sub-second nanoseconds in production; tests pass a
/// fixed value) rather than an internal RNG call — enough variance to avoid
/// synchronized retries across concurrent requests, not cryptographic
/// randomness.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// # use dtrpg_ui::services::retry::backoff_delay;
/// let delay = backoff_delay(1, 0, 2, 30);
/// assert_eq!(delay, Duration::from_secs(2));
/// ```
#[must_use]
pub fn backoff_delay(attempt: u32, jitter_source: u64, base_secs: u64, max_secs: u64) -> Duration {
    let shift = attempt.saturating_sub(1).min(63);
    let exp_secs = base_secs.saturating_mul(1u64 << shift);
    let capped_secs = exp_secs.min(max_secs);

    let jitter_range = capped_secs / 4;
    let jittered_secs = if jitter_range == 0 {
        capped_secs
    }
    else {
        let offset = (jitter_source % (jitter_range * 2 + 1)) as i64 - jitter_range as i64;
        (capped_secs as i64 + offset).clamp(0, max_secs as i64) as u64
    };

    Duration::from_secs(jittered_secs)
}

/// Fixed retry policy for a [`retry_with_backoff`] call site.
#[derive(Clone, Copy, Debug)]
pub struct RetryConfig {
    /// Total attempts including the first, non-retry attempt.
    pub max_attempts: u32,
    /// Base delay (seconds) for the first retry.
    pub base_secs:    u64,
    /// Maximum delay (seconds) any single retry will wait.
    pub max_secs:     u64,
}

/// Callback invoked once per retry attempt with the attempt number, the
/// backoff delay before the next attempt, and the error that triggered the
/// retry.
pub type OnRetry<'a, E> = &'a mut dyn FnMut(u32, Duration, &E);

/// Retries `operation` up to `config.max_attempts` times, waiting a
/// [`backoff_delay`] between attempts, as long as `is_retryable` returns
/// `true` for the failure and `cancel` has not been set.
///
/// `extract_retry_after(&error)` is consulted once per failed attempt; when
/// it returns `Some(duration)`, that server-specified duration is used as the
/// wait instead of the computed [`backoff_delay`]. Call sites with no such
/// signal pass `|_| None` to preserve the default exponential-backoff
/// behavior.
///
/// `on_retry(attempt, delay, &error)` is invoked once per retry, right before
/// the backoff wait begins, mirroring `list_items_paged`'s `on_page`/`on_total`
/// callback idiom used elsewhere in this codebase.
///
/// # Errors
///
/// Returns the last attempt's error if every attempt fails, if `is_retryable`
/// returns `false` for an error, or if `cancel` is observed set (whether
/// before an attempt starts or mid-backoff).
pub fn retry_with_backoff<T, E>(config: RetryConfig, cancel: &AtomicBool,
                                mut operation: impl FnMut() -> Result<T, E>,
                                is_retryable: impl Fn(&E) -> bool,
                                extract_retry_after: impl Fn(&E) -> Option<Duration>,
                                mut on_retry: Option<OnRetry<'_, E>>)
                                -> Result<T, E> {
    let mut attempt = 1;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) => {
                let retryable = is_retryable(&error) && !cancel.load(Ordering::SeqCst);
                if !retryable || attempt >= config.max_attempts {
                    return Err(error);
                }

                let delay = extract_retry_after(&error).unwrap_or_else(|| {
                                                           backoff_delay(attempt,
                                                                         jitter_source_now(),
                                                                         config.base_secs,
                                                                         config.max_secs)
                                                       });
                if let Some(on_retry) = on_retry.as_deref_mut() {
                    on_retry(attempt, delay, &error);
                }

                if !wait_cancelable(delay, cancel) {
                    return Err(error);
                }
                attempt += 1;
            }
        }
    }
}

/// Sleeps for `delay` in [`BACKOFF_TICK`] increments, checking `cancel`
/// between ticks. Returns `false` if cancellation was observed before the
/// full delay elapsed.
fn wait_cancelable(delay: Duration, cancel: &AtomicBool) -> bool {
    let mut remaining = delay;
    while remaining > Duration::ZERO {
        if cancel.load(Ordering::SeqCst) {
            return false;
        }
        let tick = remaining.min(BACKOFF_TICK);
        std::thread::sleep(tick);
        remaining -= tick;
    }
    !cancel.load(Ordering::SeqCst)
}

/// Sub-second nanoseconds of the current time, used as a deterministic jitter
/// source in production. Tests pass fixed values to [`backoff_delay`]
/// directly rather than calling this.
fn jitter_source_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH)
                     .map(|d| u64::from(d.subsec_nanos()))
                     .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use super::*;

    #[test]
    fn backoff_delay_has_no_jitter_when_range_rounds_to_zero() {
        // capped_secs=2 → jitter_range = 2/4 = 0, so the result is exact.
        assert_eq!(backoff_delay(1, 0, 2, 30), Duration::from_secs(2));
        assert_eq!(backoff_delay(1, 999_999, 2, 30), Duration::from_secs(2));
    }

    #[test]
    fn backoff_delay_grows_with_attempt_before_cap() {
        let d1 = backoff_delay(1, 0, 2, 30).as_secs();
        let d2 = backoff_delay(2, 0, 2, 30).as_secs();
        let d3 = backoff_delay(3, 0, 2, 30).as_secs();
        assert!(d1 < d2);
        assert!(d2 < d3);
    }

    #[test]
    fn backoff_delay_never_exceeds_max_regardless_of_jitter() {
        for jitter_source in [0, 1, 100, 999_999, u64::MAX] {
            assert!(backoff_delay(20, jitter_source, 2, 30).as_secs() <= 30);
        }
    }

    #[test]
    fn backoff_delay_is_deterministic_for_same_jitter_source() {
        assert_eq!(backoff_delay(3, 12345, 2, 30),
                   backoff_delay(3, 12345, 2, 30));
    }

    #[test]
    fn backoff_delay_jitter_stays_within_configured_range() {
        // attempt=3, base=2, max=30 → pre-jitter capped delay is 8s, +/-25% is
        // [6, 10].
        for jitter_source in [0, 1, 100, 999_999] {
            let delay = backoff_delay(3, jitter_source, 2, 30).as_secs();
            assert!((6..=10).contains(&delay),
                    "delay {delay} out of expected range");
        }
    }

    #[test]
    fn retry_with_backoff_succeeds_without_retry() {
        let cancel = AtomicBool::new(false);
        let config = RetryConfig { max_attempts: 3,
                                   base_secs:    0,
                                   max_secs:     0, };
        let result: Result<u32, &str> =
            retry_with_backoff(config, &cancel, || Ok(42), |_| true, |_| None, None);
        assert_eq!(result, Ok(42));
    }

    #[test]
    fn retry_with_backoff_retries_up_to_max_attempts() {
        let cancel = AtomicBool::new(false);
        let attempts = AtomicU32::new(0);
        let config = RetryConfig { max_attempts: 3,
                                   base_secs:    0,
                                   max_secs:     0, };
        let result: Result<(), &str> = retry_with_backoff(config,
                                                          &cancel,
                                                          || {
                                                              attempts.fetch_add(1,
                                                                                 Ordering::SeqCst);
                                                              Err("transient")
                                                          },
                                                          |_| true,
                                                          |_| None,
                                                          None);
        assert_eq!(result, Err("transient"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn retry_with_backoff_stops_when_not_retryable() {
        let cancel = AtomicBool::new(false);
        let attempts = AtomicU32::new(0);
        let config = RetryConfig { max_attempts: 5,
                                   base_secs:    0,
                                   max_secs:     0, };
        let result: Result<(), &str> = retry_with_backoff(config,
                                                          &cancel,
                                                          || {
                                                              attempts.fetch_add(1,
                                                                                 Ordering::SeqCst);
                                                              Err("fatal")
                                                          },
                                                          |_| false,
                                                          |_| None,
                                                          None);
        assert_eq!(result, Err("fatal"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn retry_with_backoff_stops_when_cancelled_before_next_attempt() {
        let cancel = AtomicBool::new(false);
        let attempts = AtomicU32::new(0);
        let config = RetryConfig { max_attempts: 5,
                                   base_secs:    0,
                                   max_secs:     0, };
        let result: Result<(), &str> = retry_with_backoff(config,
                                                          &cancel,
                                                          || {
                                                              attempts.fetch_add(1,
                                                                                 Ordering::SeqCst);
                                                              cancel.store(true, Ordering::SeqCst);
                                                              Err("transient")
                                                          },
                                                          |_| true,
                                                          |_| None,
                                                          None);
        assert_eq!(result, Err("transient"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn retry_with_backoff_invokes_on_retry_callback() {
        let cancel = AtomicBool::new(false);
        let retry_calls = AtomicU32::new(0);
        let config = RetryConfig { max_attempts: 3,
                                   base_secs:    0,
                                   max_secs:     0, };
        let mut on_retry = |attempt: u32, _delay: Duration, _error: &&str| {
            assert!(attempt >= 1);
            retry_calls.fetch_add(1, Ordering::SeqCst);
        };
        let result: Result<(), &str> = retry_with_backoff(config,
                                                          &cancel,
                                                          || Err("transient"),
                                                          |_| true,
                                                          |_| None,
                                                          Some(&mut on_retry));
        assert_eq!(result, Err("transient"));
        assert_eq!(retry_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn retry_with_backoff_waits_extract_retry_after_duration_instead_of_backoff() {
        let cancel = AtomicBool::new(false);
        let config = RetryConfig { max_attempts: 2,
                                   base_secs:    100,
                                   max_secs:     200, };
        let mut recorded_delay = None;
        let mut on_retry = |_attempt: u32, delay: Duration, _error: &&str| {
            recorded_delay = Some(delay);
        };
        let result: Result<(), &str> = retry_with_backoff(config,
                                                          &cancel,
                                                          || Err("rate_limited"),
                                                          |_| true,
                                                          |_| Some(Duration::from_millis(5)),
                                                          Some(&mut on_retry));
        assert_eq!(result, Err("rate_limited"));
        assert_eq!(recorded_delay, Some(Duration::from_millis(5)));
    }

    #[test]
    fn retry_with_backoff_falls_back_to_computed_backoff_when_extract_retry_after_is_none() {
        let cancel = AtomicBool::new(false);
        let config = RetryConfig { max_attempts: 2,
                                   base_secs:    0,
                                   max_secs:     0, };
        let mut recorded_delay = None;
        let mut on_retry = |_attempt: u32, delay: Duration, _error: &&str| {
            recorded_delay = Some(delay);
        };
        let result: Result<(), &str> = retry_with_backoff(config,
                                                          &cancel,
                                                          || Err("transient"),
                                                          |_| true,
                                                          |_| None,
                                                          Some(&mut on_retry));
        assert_eq!(result, Err("transient"));
        assert_eq!(recorded_delay, Some(Duration::ZERO));
    }
}
