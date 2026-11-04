//! Remembers that the hint about pictures was shown, so it is shown only once.

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::Path;

/// The file whose being there means the hint was already shown.
pub const IMAGE_HINT_FILE: &str = "image-hint-shown";

/// Returns whether the hint should be shown now, and notes that it was.
///
/// Making the file only succeeds if it is not there yet, so when several windows open at
/// once, only one of them shows the hint.
pub fn first_time(data_folder: &Path) -> Result<bool> {
    fs::create_dir_all(data_folder).context("could not make the data folder")?;
    let path = data_folder.join(IMAGE_HINT_FILE);
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error).context("could not note that the picture hint was shown"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A temporary folder that is removed when the test ends.
    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Result<Self> {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("meowsh-hint-{}-{unique}", std::process::id()));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_hint_is_shown_only_the_first_time() -> Result<()> {
        let directory = TestDirectory::new()?;
        // A folder that does not exist yet, as on a first run.
        let data_folder = directory.0.join("meowsh");

        assert!(first_time(&data_folder)?);
        assert!(data_folder.join(IMAGE_HINT_FILE).exists());
        assert!(!first_time(&data_folder)?);
        Ok(())
    }

    #[test]
    fn a_data_folder_that_cannot_be_made_is_an_error() {
        // /dev/null is a file, so nothing can be made inside it.
        assert!(first_time(Path::new("/dev/null/meowsh")).is_err());
    }
}
