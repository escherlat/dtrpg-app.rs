mod app;
mod constants;
mod logging;
mod services;

fn main() {
    let log_guards = logging::init();

    // Verification-only escape hatch for confirming a build's Sentry
    // wiring end-to-end (crash-reporting change, task 6.3): emits one
    // ERROR-level event and exits immediately, without launching the GUI,
    // so `DTRPG_SENTRY_DSN=<dsn> cargo run --features dtrpg-core/sentry --
    // --trigger-test-error` can be checked against the Sentry project
    // directly. Dropping `log_guards` on return flushes the event (and any
    // buffered log lines) before the process exits.
    if std::env::args().any(|arg| arg == "--trigger-test-error") {
        tracing::error!("manual Sentry verification trigger (--trigger-test-error)");
        drop(log_guards);
        return;
    }

    app::run();
}
