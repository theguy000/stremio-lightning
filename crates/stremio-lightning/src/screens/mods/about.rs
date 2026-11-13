//! The About page: what this build is, where it keeps themes, what built-in plugins and local
//! themes are present, and the button that writes a diagnostics file.

use std::path::{Path, PathBuf};

use stremio_lightning_core::mods::{self, ModType};

use super::catalogue::{self, Kind};
use crate::plumbing::logs::{Entry, Logs};
use crate::screens::settings::LocalExtras;

/// The version this binary was built from.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The commit this binary was built at, or `dev` for a build that did not come from CI.
pub const BUILD: &str = match option_env!("GITHUB_SHA") {
    Some(sha) => sha,
    None => "dev",
};
/// The line about the licence, which is the file the source ships under.
pub const LICENSE: &str =
    "Stremio Lightning is free software under the MIT licence. It is not affiliated with Stremio.";
/// How many of the newest records the diagnostics file carries.
pub const RECORDS: usize = 200;
/// The folder the diagnostics file is written to, under the app's data folder.
pub const FOLDER: &str = "diagnostics";
/// What the page says above its list.
pub const NOTE: &str = "What this build is and what it keeps. Refresh reads the themes folder again; Export diagnostics writes one text file into the app's data folder and shows where.";

/// One line of the page: what it is about, and what it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub value: String,
}

/// Everything the About page shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Data {
    /// The version, the build and the platform, one line each.
    pub build: Vec<Line>,
    /// The themes folder.
    pub places: Vec<Line>,
    /// Built-in plugin and local `.theme.css` counts.
    pub counts: Vec<Line>,
    /// Enabled built-in plugins and local themes.
    pub enabled: Vec<Line>,
    pub license: String,
    /// Where the diagnostics file was written, empty until one is.
    pub export: String,
}

/// What the counts are read from: the app's data folder and the shell's settings.
pub struct Sources<'a> {
    pub data_dir: &'a Path,
    pub extras: &'a LocalExtras,
}

/// Everything the About page shows, read now.
pub fn data(sources: &Sources<'_>) -> Data {
    let plugins: Vec<catalogue::ModCard> =
        catalogue::cards(sources.data_dir, Kind::Plugin, sources.extras).unwrap_or_default();
    let themes: Vec<catalogue::ModCard> =
        catalogue::cards(sources.data_dir, Kind::Theme, sources.extras).unwrap_or_default();
    let enabled: Vec<String> = plugins
        .iter()
        .filter(|card| card.enabled)
        .map(|card| card.filename.clone())
        .collect();
    let themes_on: Vec<String> = themes
        .iter()
        .filter(|card| card.enabled)
        .map(|card| card.filename.clone())
        .collect();

    Data {
        build: vec![
            line("Version", VERSION),
            line("Build", BUILD),
            line(
                "Platform",
                format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH),
            ),
        ],
        places: vec![line(
            "Themes folder",
            mods::mods_dir(sources.data_dir, ModType::Theme)
                .display()
                .to_string(),
        )],
        counts: vec![
            line("Built-in plugins", plugins.len().to_string()),
            line(
                "Built-in plugins enabled",
                plugins
                    .iter()
                    .filter(|card| card.enabled)
                    .count()
                    .to_string(),
            ),
            line("Themes installed", themes.len().to_string()),
            line(
                "Themes enabled",
                themes
                    .iter()
                    .filter(|card| card.enabled)
                    .count()
                    .to_string(),
            ),
        ],
        enabled: vec![
            line("Enabled plugins", listed(&enabled)),
            line("Enabled themes", listed(&themes_on)),
        ],
        license: LICENSE.to_owned(),
        export: String::new(),
    }
}

fn line(label: &str, value: impl Into<String>) -> Line {
    Line {
        label: label.to_owned(),
        value: value.into(),
    }
}

/// A list of file names as one line: "none", the one name, or the names after a comma.
fn listed(names: &[String]) -> String {
    match names {
        [] => "none".to_owned(),
        [one] => one.clone(),
        names => names.join(", "),
    }
}

/// The diagnostics file's text: the build, the platform, the counts, the enabled lists, and the
/// newest `records` records.
pub fn report(data: &Data, records: &[Entry]) -> String {
    let mut text = String::from("Stremio Lightning diagnostics\n\n");
    text.push_str(
        "This file holds what the app's About and Logs pages show. Nothing in it was uploaded.\n\n",
    );
    for section in [&data.build, &data.places, &data.counts, &data.enabled] {
        for entry in section {
            text.push_str(&format!("{}: {}\n", entry.label, entry.value));
        }
        text.push('\n');
    }
    text.push_str(&format!("{}\n\n", data.license));
    text.push_str(&format!("Log records, newest first (up to {RECORDS}):\n"));
    if records.is_empty() {
        text.push_str("(none)\n");
    }
    for entry in records {
        text.push_str(&entry.written());
        text.push('\n');
    }
    text
}

/// What the About page says once a diagnostics file has been written to `path`: its own lines and
/// counts as they were when it was written, and the path. Empty `path` is the page as `data` would
/// build it.
pub fn exported(mut data: Data, path: &Path) -> Data {
    data.export = path.display().to_string();
    data
}

/// The path the next diagnostics file would go to, for the page to copy before one is written:
/// the folder, because the file's name carries the second it is written.
pub fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join(FOLDER)
}

/// Writes the diagnostics file into `<data_dir>/diagnostics/`, named by the second the app wrote
/// it, and returns its path and what it says. The file is text, and nothing leaves the computer.
///
/// # Errors
/// Returns a sentence for the page when the folder or the file cannot be written.
pub fn export(
    data: &Data,
    logs: &Logs,
    data_dir: &Path,
    at: u64,
) -> Result<(PathBuf, String), String> {
    let records: Vec<Entry> = logs.entries().into_iter().rev().take(RECORDS).collect();
    let text = report(data, &records);
    let folder: PathBuf = data_dir.join(FOLDER);
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("{} could not be made: {error}", folder.display()))?;
    let path = folder.join(format!("diagnostics-{at}.txt"));
    std::fs::write(&path, &text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    Ok((path, text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plumbing::logs::{Level, Source};

    fn labels(section: &[Line]) -> Vec<&str> {
        section.iter().map(|line| line.label.as_str()).collect()
    }

    #[test]
    fn a_list_of_names_says_none_one_or_all_of_them() {
        assert_eq!(listed(&[]), "none");
        assert_eq!(listed(&["a".to_owned()]), "a");
        assert_eq!(listed(&["a".to_owned(), "b".to_owned()]), "a, b");
    }

    #[test]
    fn the_report_carries_the_counts_the_lists_and_the_newest_records() {
        let mut data = Data {
            counts: vec![line("Built-in plugins", "1")],
            enabled: vec![line("Enabled plugins", "builtin:aniskip")],
            license: LICENSE.to_owned(),
            ..Data::default()
        };
        data.build = vec![line("Version", "9.9.9")];
        let logs = Logs::new();
        // The recorder keeps CAPACITY records; ask the page for its own shorter list.
        for index in 0..crate::plumbing::logs::CAPACITY {
            logs.record(Source::App, "", Level::Info, format!("line {index}"));
        }
        let records: Vec<Entry> = logs.entries().into_iter().rev().take(RECORDS).collect();
        let text = report(&data, &records);
        assert!(text.contains("Version: 9.9.9"));
        assert!(text.contains("Built-in plugins: 1"));
        assert!(text.contains("Enabled plugins: builtin:aniskip"));
        assert!(text.contains("MIT licence"));
        let first = text
            .find("line 1999")
            .expect("the newest record is in the file");
        let older = text
            .find("line 1800")
            .expect("a record within RECORDS is in the file");
        assert!(first < older, "the newest is first");
        assert!(
            !text.contains("line 1799"),
            "the kept records stop at RECORDS"
        );
        assert_eq!(text.matches("Info [App]").count(), RECORDS);
    }

    #[test]
    fn the_two_lists_of_counts_are_the_pages_own() {
        let data = Data {
            counts: vec![line("Built-in plugins", "1")],
            ..Data::default()
        };
        assert_eq!(labels(&data.counts), ["Built-in plugins"]);
        assert!(data.places.is_empty(), "nothing was read for this one");
    }
}
