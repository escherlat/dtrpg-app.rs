//! Logging initialisation: WARN+ to stderr and to a rolling daily log file.
//!
//! The log directory follows platform conventions:
//! - macOS:   `~/Library/Logs/com.pilgrimagesoftware.dtrpg/`
//! - Linux:   `$XDG_DATA_HOME/com.pilgrimagesoftware.dtrpg/logs/`
//! - Windows: `%APPDATA%\com.pilgrimagesoftware.dtrpg\logs\`
//!
//! The default filter level is `WARN`. Set `RUST_LOG` to override (e.g.
//! `RUST_LOG=debug cargo run` for verbose output during development). The
//! `gpui` crate's own internal logging (window/accessibility-tree chatter)
//! is always capped at `INFO`, regardless of `RUST_LOG`, since its DEBUG
//! output is high-volume and rarely useful outside gpui development itself.
//!
//! When compiled with the `sentry` feature and `DTRPG_SENTRY_DSN` is set at
//! runtime, ERROR-level `tracing` events are additionally forwarded to
//! Sentry as issues (WARN and below are attached as breadcrumbs). Source
//! builds without the feature, or any build without a configured DSN, never
//! initialise Sentry; [`init`] logs exactly which of these cases applies.

use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

/// Guards that must be kept alive for the lifetime of the process.
///
/// Dropping this early stops the background log-file flush thread and, when
/// Sentry is active, may drop buffered events before they are sent.
pub struct LogGuards {
    _worker: Option<WorkerGuard>,
    #[cfg(feature = "sentry")]
    _sentry: Option<sentry::ClientInitGuard>,
}

/// Result of attempting to initialise Sentry, used only for the one-line
/// startup status log.
enum SentryStatus {
    // Only constructed by `init_sentry_client`, compiled under the `sentry`
    // feature; allow dead-code lint in default (no-feature) builds.
    #[cfg_attr(not(feature = "sentry"), allow(dead_code))]
    Active,
    #[cfg_attr(not(feature = "sentry"), allow(dead_code))]
    DisabledNoDsn,
    // Only constructed when the `sentry` feature is *not* compiled in; allow
    // dead-code lint in `sentry`-feature builds where this arm is unused.
    #[cfg_attr(feature = "sentry", allow(dead_code))]
    DisabledNotCompiled,
}

impl SentryStatus {
    fn log(&self) {
        match self {
            Self::Active => tracing::info!("sentry crash reporting: active"),
            Self::DisabledNoDsn => {
                tracing::info!("sentry crash reporting: disabled (no DSN configured)");
            }
            Self::DisabledNotCompiled => {
                tracing::info!("sentry crash reporting: disabled (not compiled into this build)");
            }
        }
    }
}

/// Initialises the global tracing subscriber with console, file, and
/// (optionally) Sentry layers.
///
/// Returns [`LogGuards`] that **must be kept alive** for the lifetime of the
/// process; dropping them early will stop the background flush thread(s) and
/// may lose buffered log lines or Sentry events.
///
/// If the log directory cannot be created the function falls back to
/// console-only logging.
pub fn init() -> LogGuards {
    let log_dir = platform_log_dir();

    #[cfg(feature = "sentry")]
    let (sentry_guard, sentry_status) = init_sentry_client();
    #[cfg(not(feature = "sentry"))]
    let sentry_status = SentryStatus::DisabledNotCompiled;
    let sentry_active = matches!(sentry_status, SentryStatus::Active);

    let guards = if let Err(e) = std::fs::create_dir_all(&log_dir) {
        // Can't create log dir — set up console-only and continue.
        let filter = build_filter();
        tracing_subscriber::registry().with(filter)
                                      .with(fmt::layer().with_writer(std::io::stderr).compact())
                                      .with(sentry_layer(sentry_active))
                                      .init();
        eprintln!("warning: could not create log directory {}: {e}",
                  log_dir.display());
        LogGuards { _worker:                            None,
                    #[cfg(feature = "sentry")]
                    _sentry:                            sentry_guard, }
    }
    else {
        let file_appender = tracing_appender::rolling::daily(&log_dir, "libri.log");
        let (non_blocking, worker_guard) = tracing_appender::non_blocking(file_appender);

        let filter = build_filter();

        tracing_subscriber::registry().with(filter)
                                      .with(fmt::layer().with_writer(std::io::stderr).compact())
                                      .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
                                      .with(sentry_layer(sentry_active))
                                      .init();

        LogGuards { _worker:                            Some(worker_guard),
                    #[cfg(feature = "sentry")]
                    _sentry:                            sentry_guard, }
    };

    sentry_status.log();
    guards
}

/// Reads Sentry configuration from the runtime environment, falling back to
/// values embedded at compile time by `build.rs` (from CI's build-time
/// environment), and initialises the client if a DSN is present from either
/// source.
///
/// Returns the client guard (kept alive by the caller) and the resulting
/// status for logging. The client guard is `None` unless a DSN was supplied
/// and the client initialised successfully.
#[cfg(feature = "sentry")]
fn init_sentry_client() -> (Option<sentry::ClientInitGuard>, SentryStatus) {
    let dsn = runtime_or_builtin(crate::constants::SENTRY_DSN_ENV,
                                 option_env!("DTRPG_SENTRY_DSN_BUILTIN"));
    let environment = runtime_or_builtin(
        crate::constants::SENTRY_ENVIRONMENT_ENV,
        option_env!("DTRPG_SENTRY_ENVIRONMENT_BUILTIN"),
    )
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| crate::constants::SENTRY_DEFAULT_ENVIRONMENT.to_string());
    let release = runtime_or_builtin(
        crate::constants::SENTRY_RELEASE_ENV,
        option_env!("DTRPG_SENTRY_RELEASE_BUILTIN"),
    )
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    init_sentry_with(dsn.unwrap_or_default(), environment, release)
}

/// Reads a runtime environment variable, falling back to a value baked in at
/// compile time (via `build.rs`) if the runtime variable is unset or empty.
/// Returns `None` if neither source has a non-empty value.
#[cfg(feature = "sentry")]
fn runtime_or_builtin(env_var: &str, builtin: Option<&str>) -> Option<String> {
    std::env::var(env_var).ok()
                          .filter(|s| !s.is_empty())
                          .or_else(|| builtin.filter(|s| !s.is_empty()).map(str::to_string))
}

/// Initialises the Sentry client with the given DSN/environment/release.
///
/// An empty `dsn` is treated by `sentry::init` as "no DSN configured" (see
/// [`sentry::IntoDsn`]) and results in a disabled, inert client — this
/// function reflects that back as [`SentryStatus::DisabledNoDsn`] rather
/// than attempting any network access. Split out from
/// [`init_sentry_client`] so it can be exercised directly in tests without
/// mutating process-wide environment state.
#[cfg(feature = "sentry")]
fn init_sentry_with(dsn: String, environment: String, release: String)
                    -> (Option<sentry::ClientInitGuard>, SentryStatus) {
    let guard = sentry::init((dsn,
                              sentry::ClientOptions::new().environment(environment)
                                                          .release(release)));

    if guard.is_enabled() {
        (Some(guard), SentryStatus::Active)
    }
    else {
        (None, SentryStatus::DisabledNoDsn)
    }
}

/// Builds the `tracing` layer that forwards events to Sentry, generic over
/// the concrete subscriber stack it's attached to (which differs between the
/// two branches in [`init`]).
///
/// Returns `None` when `active` is `false`, or unconditionally when the
/// `sentry` feature is not compiled in.
#[cfg(feature = "sentry")]
fn sentry_layer<S>(active: bool) -> Option<sentry_tracing::SentryLayer<S>>
    where S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> {
    active.then(sentry_tracing::layer)
}

/// No-op stand-in for [`sentry_tracing::SentryLayer`] used when the `sentry`
/// feature is not compiled in. Generic over `S` (like the real layer) so
/// type inference at each `.with(sentry_layer(...))` call site resolves the
/// same way regardless of which feature is active.
#[cfg(not(feature = "sentry"))]
struct NoopLayer<S>(std::marker::PhantomData<S>);

#[cfg(not(feature = "sentry"))]
impl<S> tracing_subscriber::Layer<S> for NoopLayer<S> where S: tracing::Subscriber {}

#[cfg(not(feature = "sentry"))]
fn sentry_layer<S>(_active: bool) -> Option<NoopLayer<S>>
    where S: tracing::Subscriber {
    None
}

/// Builds the log filter: `RUST_LOG` if set, otherwise `warn`, with an
/// always-applied `gpui=info` directive layered on top.
///
/// `gpui` logs its own internal window/accessibility-tree chatter at DEBUG
/// (e.g. "Sending a11y tree update"), which floods the log whenever
/// verbosity is raised (e.g. `RUST_LOG=debug`) to see this app's own
/// debug-level logging. Requiring at least INFO for the `gpui` target keeps
/// its warnings and errors (e.g. a window `RefCell` panic) visible while
/// suppressing that routine noise — appended after the base filter so it
/// takes effect regardless of whether `RUST_LOG` is set.
fn build_filter() -> EnvFilter {
    let base = std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string());
    EnvFilter::try_new(format!("{base},gpui=info")).unwrap_or_else(|_| EnvFilter::new("warn,gpui=info"))
}

fn platform_log_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Logs/com.pilgrimagesoftware.dtrpg"))
                        .unwrap_or_else(|| PathBuf::from("/tmp/com.pilgrimagesoftware.dtrpg"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        dirs::data_local_dir()
            .map(|d| d.join("com.pilgrimagesoftware.dtrpg/logs"))
            .unwrap_or_else(|| PathBuf::from("/tmp/com.pilgrimagesoftware.dtrpg/logs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without the `sentry` feature compiled in, status resolution must
    /// always report "not compiled", independent of any configuration.
    #[test]
    #[cfg(not(feature = "sentry"))]
    fn sentry_disabled_when_feature_not_compiled() {
        let status = SentryStatus::DisabledNotCompiled;
        assert!(matches!(status, SentryStatus::DisabledNotCompiled));
    }

    /// With the `sentry` feature compiled in but an empty DSN, initialisation
    /// must resolve to "disabled - no DSN" and return no client guard.
    #[test]
    #[cfg(feature = "sentry")]
    fn sentry_disabled_when_dsn_missing() {
        let (guard, status) =
            init_sentry_with(String::new(), "test".to_string(), "0.0.0".to_string());
        assert!(guard.is_none());
        assert!(matches!(status, SentryStatus::DisabledNoDsn));
    }

    /// With the `sentry` feature compiled in and a DSN set, initialisation
    /// must resolve to "active" and return a client guard, without requiring
    /// network access (DSN parsing/client construction is local).
    #[test]
    #[cfg(feature = "sentry")]
    fn sentry_active_when_dsn_present() {
        let (guard, status) = init_sentry_with("https://public@sentry.example.test/1".to_string(),
                                               "test".to_string(),
                                               "0.0.0".to_string());
        assert!(guard.is_some());
        assert!(matches!(status, SentryStatus::Active));
    }
}
