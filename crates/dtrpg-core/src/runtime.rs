//! The process-wide Tokio runtime.
//!
//! Every service that drives async SDK calls from synchronous code shares this
//! one runtime instead of building its own. The runtime is created the first
//! time [`handle`] is called and lives for the rest of the process.

use std::{io, sync::OnceLock};

use tokio::runtime::{Builder, Handle, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Returns a handle to the shared runtime, creating the runtime on first use.
///
/// A [`Handle`] can run futures with [`Handle::block_on`] from any thread that
/// is not itself inside an async context, the same as the runtime it belongs
/// to.
///
/// # Errors
///
/// Returns an [`io::Error`] if the runtime cannot be started.
///
/// # Examples
///
/// ```
/// let handle = dtrpg_core::runtime::handle()?;
/// let value = handle.block_on(async { 1 + 1 });
/// assert_eq!(value, 2);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn handle() -> io::Result<Handle> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime.handle().clone());
    }

    // If two threads get here together, the loser's runtime is dropped unused.
    let built = Builder::new_multi_thread().enable_all().build()?;
    Ok(RUNTIME.get_or_init(|| built).handle().clone())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn handle_runs_futures() {
        let value = handle().unwrap().block_on(async { 21 * 2 });
        assert_eq!(value, 42);
    }

    #[test]
    fn handles_share_one_runtime() {
        let first = handle().unwrap();
        let second = handle().unwrap();
        assert_eq!(first.id(), second.id());
    }

    #[test]
    fn handle_is_shared_across_threads() {
        let here = handle().unwrap().id();
        let worker = std::thread::spawn(|| handle().unwrap().id());
        let there = worker.join().unwrap();
        assert_eq!(here, there);
    }
}
