//! Captures build-time identifying information (git commit, build date,
//! target triple) as compile-time `rustc-env` variables, so the About
//! settings section can show exactly which build is running.
//!
//! `dtrpg-core` (the binary) depends on `dtrpg-ui` (the library) — not the
//! reverse — so this crate needs its own build script rather than reusing
//! `dtrpg-core/build.rs`'s Sentry-config forwarding.

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    let git_hash =
        std::process::Command::new("git").args(["rev-parse", "--short", "HEAD"])
                                         .output()
                                         .ok()
                                         .filter(|output| output.status.success())
                                         .map(|output| {
                                             String::from_utf8_lossy(&output.stdout).trim()
                                                                                    .to_string()
                                         })
                                         .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=DTRPG_GIT_HASH={git_hash}");

    println!("cargo:rustc-env=DTRPG_BUILD_DATE={}", build_date_utc());

    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=DTRPG_TARGET={target}");
}

/// Returns today's UTC date as `YYYY-MM-DD`, computed from
/// [`std::time::SystemTime`] without shelling out to `date` (which isn't
/// available when cross-compiling for Windows).
///
/// Uses Howard Hinnant's `civil_from_days` algorithm to convert
/// days-since-epoch to a calendar date: <https://howardhinnant.github.io/date_algorithms.html>.
fn build_date_utc() -> String {
    let days_since_epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                                                       .map(|d| d.as_secs() / 86_400)
                                                       .unwrap_or(0)
                           as i64;

    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };

    format!("{year:04}-{month:02}-{day:02}")
}
