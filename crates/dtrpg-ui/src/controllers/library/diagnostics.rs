//! Diagnostic helpers for logging catalog-load failures.

/// Captures the current backtrace and keeps only frames that mention a
/// `dtrpg_` symbol, so a logged load failure points at application code
/// rather than the gpui/runtime stack beneath it.
pub(crate) fn app_backtrace() -> String {
    let bt = std::backtrace::Backtrace::capture();
    if bt.status() != std::backtrace::BacktraceStatus::Captured {
        return "<set RUST_BACKTRACE=1 to capture backtraces>".to_string();
    }
    let full = format!("{bt}");
    let mut out: Vec<&str> = Vec::new();
    let mut take_location = false;
    for line in full.lines() {
        if line.trim_start().starts_with("at ") {
            if take_location {
                out.push(line);
                take_location = false;
            }
        }
        else if line.contains("dtrpg_") {
            out.push(line);
            take_location = true;
        }
        else {
            take_location = false;
        }
    }
    if out.is_empty() {
        "<no app frames found in backtrace>".to_string()
    }
    else {
        out.join("\n")
    }
}
