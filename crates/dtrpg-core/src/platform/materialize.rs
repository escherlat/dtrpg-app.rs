//! Waits for a cloud placeholder to become a local file.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use super::{FileState, PlatformIntegration};

/// How long [`materialize_and_wait`] waits, and how often it checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterializeWait {
    /// Longest time to wait for the file to become local.
    pub timeout:       Duration,
    /// Time between checks of the file's state.
    pub poll_interval: Duration,
}

impl Default for MaterializeWait {
    fn default() -> Self {
        Self { timeout:       Duration::from_secs(120),
               poll_interval: Duration::from_millis(250), }
    }
}

/// Asks the platform to make a cloud placeholder local, then waits until it is.
///
/// A file that is already local, missing, or in a state the platform cannot
/// name is left alone and counts as done: the caller's own open reports a
/// missing file. This blocks, so call it from a background thread.
///
/// # Errors
///
/// Returns [`io::ErrorKind::Unsupported`] if the file is a placeholder and the
/// platform cannot materialize files, [`io::ErrorKind::TimedOut`] if the file
/// is still not local after `wait.timeout`, and any error the platform reports
/// while checking or requesting the file.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
///
/// use dtrpg_core::platform::{MaterializeWait, Platform, materialize_and_wait};
///
/// materialize_and_wait(&Platform, Path::new("Book.pdf"), MaterializeWait::default())?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn materialize_and_wait<P>(platform: &P, path: &Path, wait: MaterializeWait) -> io::Result<()>
    where P: PlatformIntegration + ?Sized {
    if platform.file_state(path)? == FileState::CloudOnly {
        if !platform.capabilities().materialize {
            return Err(io::Error::new(io::ErrorKind::Unsupported,
                                      "this platform cannot materialize cloud-backed files"));
        }
        platform.materialize(path)?;
    }

    let deadline = Instant::now() + wait.timeout;
    while matches!(platform.file_state(path)?,
                   FileState::CloudOnly | FileState::Downloading)
    {
        if Instant::now() >= deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut,
                                      "the file did not become local in time"));
        }
        std::thread::sleep(wait.poll_interval);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::platform::{Capabilities, PlatformKind};

    /// A platform whose file reports each scripted state in turn, then the
    /// last.
    struct Scripted {
        states:       Mutex<Vec<FileState>>,
        can_download: bool,
        requests:     AtomicU32,
    }

    impl Scripted {
        fn new(states: &[FileState], can_download: bool) -> Self {
            Self { states: Mutex::new(states.iter().rev().copied().collect()),
                   can_download,
                   requests: AtomicU32::new(0) }
        }
    }

    impl PlatformIntegration for Scripted {
        fn kind(&self) -> PlatformKind {
            PlatformKind::Linux
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities { materialize: self.can_download,
                           ..Capabilities::default() }
        }

        fn file_state(&self, _path: &Path) -> io::Result<FileState> {
            let mut states = self.states.lock().unwrap();
            if states.len() > 1 {
                Ok(states.pop().unwrap_or(FileState::Unknown))
            }
            else {
                Ok(states.last().copied().unwrap_or(FileState::Unknown))
            }
        }

        fn reveal(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }

        fn materialize(&self, _path: &Path) -> io::Result<()> {
            self.requests.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    const QUICK: MaterializeWait = MaterializeWait { timeout:       Duration::from_millis(200),
                                                     poll_interval: Duration::from_millis(1), };

    #[test]
    fn a_local_file_is_not_requested() {
        let platform = Scripted::new(&[FileState::Local], true);

        assert!(materialize_and_wait(&platform, Path::new("a"), QUICK).is_ok());
        assert_eq!(platform.requests.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_missing_file_is_left_for_the_caller_to_report() {
        let platform = Scripted::new(&[FileState::Missing], true);

        assert!(materialize_and_wait(&platform, Path::new("a"), QUICK).is_ok());
        assert_eq!(platform.requests.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_placeholder_is_requested_once_and_waited_for() {
        let platform = Scripted::new(&[FileState::CloudOnly,
                                       FileState::CloudOnly,
                                       FileState::Downloading,
                                       FileState::Local],
                                     true);

        assert!(materialize_and_wait(&platform, Path::new("a"), QUICK).is_ok());
        assert_eq!(platform.requests.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_download_already_under_way_is_waited_for_without_a_new_request() {
        let platform = Scripted::new(&[FileState::Downloading, FileState::Local], true);

        assert!(materialize_and_wait(&platform, Path::new("a"), QUICK).is_ok());
        assert_eq!(platform.requests.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_platform_that_cannot_materialize_reports_it() {
        let platform = Scripted::new(&[FileState::CloudOnly], false);

        let error = materialize_and_wait(&platform, Path::new("a"), QUICK).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
        assert_eq!(platform.requests.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_file_that_never_arrives_times_out() {
        let platform = Scripted::new(&[FileState::CloudOnly], true);

        let error = materialize_and_wait(&platform, Path::new("a"), QUICK).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }
}
