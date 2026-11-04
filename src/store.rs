//! Files Meowsh keeps between runs, in the usual data folder for the system.
//!
//! * `image_hint`: remembers that the hint about pictures was shown.

pub mod image_hint;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::PathBuf;

/// The name of Meowsh's own folder inside the system's data folder.
const APP_NAME: &str = "meowsh";

/// Meowsh's data folder, such as `~/Library/Application Support/meowsh` on macOS. It may
/// not exist yet.
pub fn data_folder() -> Result<PathBuf> {
    let folders = ProjectDirs::from("", "", APP_NAME).context("could not find the home folder")?;
    Ok(folders.data_dir().to_path_buf())
}
