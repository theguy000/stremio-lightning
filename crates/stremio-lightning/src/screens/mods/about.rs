//! The About page: pure build info, themes folder display, mod counts, and diagnostics text
//! formatting. Zero disk I/O (file export lives in `plumbing::diagnostics`).

use std::path::Path;

use super::catalogue::ModCard;
pub use crate::plumbing::diagnostics::FOLDER;
use crate::plumbing::logs::Entry;
use crate::plumbing::sanitize::sanitize_text;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD: &str = match option_env!("GITHUB_SHA") {
    Some(sha) => sha,
    None => "dev",
};
pub const LICENSE: &str =
    "Stremio Lightning is free software under the MIT licence. It is not affiliated with Stremio.";
pub const RECORDS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Data {
    pub build: Vec<Line>,
    pub places: Vec<Line>,
    pub counts: Vec<Line>,
    pub enabled: Vec<Line>,
    pub license: String,
    pub export: String,
}

/// Pure About page projection from already-built plugin and theme cards.
pub fn data(themes_dir: &Path, plugins: &[ModCard], themes: &[ModCard]) -> Data {
    let enabled_plugins: Vec<String> = plugins
        .iter()
        .filter(|c| c.enabled)
        .map(|c| c.filename.clone())
        .collect();
    let enabled_themes: Vec<String> = themes
        .iter()
        .filter(|c| c.enabled)
        .map(|c| c.filename.clone())
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
        places: vec![line("Themes folder", themes_dir.display().to_string())],
        counts: vec![
            line("Built-in plugins", plugins.len().to_string()),
            line(
                "Built-in plugins enabled",
                enabled_plugins.len().to_string(),
            ),
            line("Themes installed", themes.len().to_string()),
            line("Themes enabled", enabled_themes.len().to_string()),
        ],
        enabled: vec![
            line("Enabled plugins", listed(&enabled_plugins)),
            line("Enabled themes", listed(&enabled_themes)),
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

fn listed(names: &[String]) -> String {
    match names {
        [] => "none".to_owned(),
        [one] => one.clone(),
        names => names.join(", "),
    }
}

/// Pure diagnostics report text (sanitized newest-first log records).
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
        text.push_str(&sanitize_text(&entry.written()));
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plumbing::logs::{Level, Logs, Source};

    #[test]
    fn listed_and_diagnostics_report_format_correctly() {
        assert_eq!(listed(&[]), "none");
        assert_eq!(listed(&["a".into()]), "a");
        assert_eq!(listed(&["a".into(), "b".into()]), "a, b");

        let mut d = Data {
            build: vec![line("Version", "9.9.9")],
            counts: vec![line("Built-in plugins", "1")],
            enabled: vec![line("Enabled plugins", "builtin:aniskip")],
            license: LICENSE.to_owned(),
            ..Data::default()
        };
        d.export = String::new();
        let logs = Logs::new();
        for i in 0..crate::plumbing::logs::CAPACITY {
            logs.record(Source::App, "", Level::Info, format!("line {i}"));
        }
        let records: Vec<Entry> = logs.entries().into_iter().rev().take(RECORDS).collect();
        let text = report(&d, &records);
        assert!(text.contains("Version: 9.9.9") && text.contains("MIT licence"));
        assert!(text.find("line 1999").unwrap() < text.find("line 1800").unwrap());
        assert!(!text.contains("line 1799"));
        assert_eq!(text.matches("Info [App]").count(), RECORDS);
    }
}
