//! Disk persistence for exported Mods -> About diagnostics reports (`<data_dir>/diagnostics/`).

use std::path::{Path, PathBuf};

/// Folder name under `<data_dir>` where diagnostics text files are written.
pub const FOLDER: &str = "diagnostics";

/// Writes a diagnostics report text file into `<data_dir>/diagnostics/diagnostics-{at}.txt`.
pub(crate) fn write_report(data_dir: &Path, at: u64, text: &str) -> Result<PathBuf, String> {
    let folder = data_dir.join(FOLDER);
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("{} could not be made: {error}", folder.display()))?;
    let path = folder.join(format!("diagnostics-{at}.txt"));
    std::fs::write(&path, text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    Ok(path)
}
