//! Native OS folder and file picker dialogs (`rfd`), with test spy hooks so tests never open
//! real OS dialogs. On Linux `rfd` defaults to `xdg-portal` (pure D-Bus, zero GTK3 dependency).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

type FolderSpy = Box<dyn Fn(&str, Option<&Path>) -> Option<PathBuf> + Send + Sync>;
type FileSpy = Box<dyn Fn(&str, Option<&Path>) -> Option<PathBuf> + Send + Sync>;

static FOLDER_SPY: Mutex<Option<FolderSpy>> = Mutex::new(None);
static FILE_SPY: Mutex<Option<FileSpy>> = Mutex::new(None);

pub fn watch_folder(spy: Option<FolderSpy>) {
    *FOLDER_SPY.lock().unwrap_or_else(|e| e.into_inner()) = spy;
}

pub fn watch_file(spy: Option<FileSpy>) {
    *FILE_SPY.lock().unwrap_or_else(|e| e.into_inner()) = spy;
}

pub async fn pick_folder(title: &str, start_dir: Option<&Path>) -> Option<String> {
    if let Some(spy) = FOLDER_SPY
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        return spy(title, start_dir).map(|p| p.to_string_lossy().into_owned());
    }
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    if let Some(dir) = start_dir.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    dialog
        .pick_folder()
        .await
        .map(|h| h.path().to_string_lossy().into_owned())
}

pub async fn pick_mpv_config(start_dir: Option<&Path>) -> Option<String> {
    let title = "Choose mpv configuration file";
    if let Some(spy) = FILE_SPY.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return spy(title, start_dir).map(|p| p.to_string_lossy().into_owned());
    }
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("mpv config", &["conf"])
        .add_filter("All files", &["*"]);
    if let Some(dir) = start_dir.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    dialog
        .pick_file()
        .await
        .map(|h| h.path().to_string_lossy().into_owned())
}

/// Default folder when opening the `mpv.conf` picker: parent of current config, else `{config_dir}/mpv`.
pub fn default_mpv_dir(current: Option<&str>) -> Option<PathBuf> {
    if let Some(parent) = current
        .filter(|s| !s.is_empty())
        .and_then(|s| Path::new(s).parent())
        .filter(|p| p.is_dir())
    {
        return Some(parent.to_path_buf());
    }
    dirs::config_dir()
        .map(|d| d.join("mpv"))
        .filter(|d| d.is_dir())
}
