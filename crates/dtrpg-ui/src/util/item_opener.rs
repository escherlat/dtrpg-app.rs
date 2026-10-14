//! Service for opening downloaded catalog items in the system's default
//! application.

use std::path::Path;

use thiserror::Error;

use crate::data::library::LibraryItemFile;

/// Errors that can occur when attempting to open an item.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// The file was not found at the specified path.
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// No default application is configured for this file type.
    #[error("No default application configured for this file type")]
    NoDefaultApp,

    /// The operating system failed to open the file.
    #[error("Failed to open file: {0}")]
    OsFailed(String),

    /// Multiple files exist and user selection is required.
    #[error("Multiple files require selection")]
    MultipleFilesRequireSelection,
}

/// Launches `path` in the system's default application.
///
/// Isolated from [`ItemOpener::open`] so tests never trigger a real OS-level
/// launch (which visibly opens another application, e.g. Preview for a PDF,
/// and can steal focus) — the `#[cfg(test)]` build swaps this for a no-op.
#[cfg(not(test))]
fn launch(path: &Path) -> std::io::Result<()> {
    open::that(path)
}

#[cfg(test)]
fn launch(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Service for opening files in the system's default application.
pub struct ItemOpener;

impl ItemOpener {
    /// Opens the file at the given path using the system's default application.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The file does not exist (`OpenError::FileNotFound`)
    /// - No default application is configured (`OpenError::NoDefaultApp`)
    /// - The OS fails to open the file (`OpenError::OsFailed`)
    pub fn open(path: &Path) -> Result<(), OpenError> {
        // Check if the file exists first
        if !path.exists() {
            return Err(OpenError::FileNotFound(path.display().to_string()));
        }

        // Attempt to open the file with the system's default application
        launch(path).map_err(|e| {
                        let error_msg = e.to_string();

                        // Try to classify the error
                        if error_msg.contains("no default application")
                           || error_msg.contains("no associated application")
                        {
                            OpenError::NoDefaultApp
                        }
                        else {
                            OpenError::OsFailed(error_msg)
                        }
                    })
    }

    /// Opens a catalog entry's downloaded content, given its per-item file
    /// list (`LibraryItem::files`).
    ///
    /// - Zero known files (e.g. stub/legacy items with no file breakdown):
    ///   falls back to opening `entry_dir` itself, as before.
    /// - Exactly one file: opens that specific file within `entry_dir`.
    /// - More than one file: returns
    ///   [`OpenError::MultipleFilesRequireSelection`] so the caller can route
    ///   the user to the entry's item list instead of guessing which file to
    ///   open (see `catalog-entry-detail-view`).
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::open`], plus
    /// [`OpenError::MultipleFilesRequireSelection`] for multi-item entries.
    pub fn open_item(entry_dir: &Path, files: &[LibraryItemFile]) -> Result<(), OpenError> {
        match files {
            [] => Self::open(entry_dir),
            [only] => Self::open(&entry_dir.join(only.name.as_ref())),
            _ => Err(OpenError::MultipleFilesRequireSelection),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use super::*;

    #[test]
    fn test_open_nonexistent_file() {
        let path = Path::new("/tmp/nonexistent_file_that_should_not_exist.txt");
        let result = ItemOpener::open(path);

        assert!(matches!(result, Err(OpenError::FileNotFound(_))));
    }

    #[test]
    fn test_open_existing_file() {
        // Create a temporary file
        let temp_path = std::env::temp_dir().join("dtrpg_test_file.txt");
        let Ok(mut file) = File::create(&temp_path)
        else {
            // Skip test if we can't create the temp file
            return;
        };
        if file.write_all(b"test content").is_err() {
            // Skip test if we can't write
            let _ = std::fs::remove_file(&temp_path);
            return;
        }
        drop(file);

        // `launch` is a no-op in test builds (see its `#[cfg(test)]` override
        // above), so this never actually opens the file in another
        // application — it only exercises the existence check.
        let result = ItemOpener::open(&temp_path);

        // Clean up
        let _ = std::fs::remove_file(&temp_path);

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn test_error_types_are_cloneable() {
        let err = OpenError::FileNotFound("test.txt".to_string());
        let _cloned = err.clone();
    }

    #[test]
    fn test_error_types_are_comparable() {
        let err1 = OpenError::NoDefaultApp;
        let err2 = OpenError::NoDefaultApp;
        assert_eq!(err1, err2);
    }

    fn make_file(name: &str) -> LibraryItemFile {
        LibraryItemFile { id:         name.into(),
                          index:      0,
                          name:       name.into(),
                          format:     "PDF".into(),
                          size_mb:    1.0,
                          downloaded: false, }
    }

    #[test]
    fn open_item_returns_multiple_files_require_selection_for_more_than_one_file() {
        let files = vec![make_file("book.pdf"), make_file("map.pdf")];
        let result = ItemOpener::open_item(Path::new("/tmp/does-not-matter"), &files);
        assert_eq!(result, Err(OpenError::MultipleFilesRequireSelection));
    }

    #[test]
    fn open_item_falls_back_to_directory_when_no_files_known() {
        let result = ItemOpener::open_item(Path::new("/tmp/nonexistent_entry_dir_stub"), &[]);
        assert!(matches!(result, Err(OpenError::FileNotFound(_))));
    }

    #[test]
    fn open_item_resolves_the_single_file_path_for_one_file() {
        let dir = std::env::temp_dir().join("dtrpg_open_item_single_test");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("book.pdf");
        let Ok(mut file) = File::create(&file_path)
        else {
            return;
        };
        let _ = file.write_all(b"stub");
        drop(file);

        let files = vec![make_file("book.pdf")];
        // `launch` is a no-op in test builds, so this never actually opens
        // "book.pdf" in the system's PDF viewer.
        let result = ItemOpener::open_item(&dir, &files);

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);

        assert_eq!(result, Ok(()));
    }
}
