use crate::common::{Result, write_file};
use std::{fs, path::Path};

pub fn bump_version(new_version: &str) -> Result<()> {
    bump_version_in(&crate::common::root(), new_version)
}

pub fn bump_version_in(root: &Path, new_version: &str) -> Result<()> {
    if !is_valid_semver(new_version) {
        return Err(
            format!("invalid version format: {new_version}. Must be semver like X.Y.Z").into(),
        );
    }

    let metainfo_path = root.join("assets/io.github.theguy000.stremio-lightning.metainfo.xml");
    let metainfo = fs::read_to_string(&metainfo_path)
        .map_err(|error| format!("failed to read {}: {error}", metainfo_path.display()))?;
    if !has_dated_metainfo_release(&metainfo, new_version) {
        return Err(format!(
            "missing dated AppStream release {new_version}. Add it to assets/io.github.theguy000.stremio-lightning.metainfo.xml before tagging."
        )
        .into());
    }

    println!("Bumping version to {new_version}...");

    let workspace_toml = root.join("Cargo.toml");
    if workspace_toml.is_file() {
        update_toml_section_version(&workspace_toml, "[workspace.package]", new_version, root)?;
    }

    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        let mut cargo_files: Vec<_> = fs::read_dir(&crates_dir)?
            .filter_map(|entry| entry.ok().map(|e| e.path().join("Cargo.toml")))
            .filter(|path| path.is_file())
            .collect();
        cargo_files.sort();
        for cargo_toml in cargo_files {
            update_toml_section_version(&cargo_toml, "[package]", new_version, root)?;
        }
    }

    let info_plist = root.join("assets/macos/Info.plist");
    if info_plist.is_file() {
        update_info_plist_version(&info_plist, new_version, root)?;
    }

    Ok(())
}

pub fn is_valid_semver(version: &str) -> bool {
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };

    let mut parts = core.split('.');
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let valid_core = matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(a), Some(b), Some(c), None) if digits(a) && digits(b) && digits(c)
    );
    valid_core
        && pre.is_none_or(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        })
}

pub fn has_dated_metainfo_release(metainfo: &str, version: &str) -> bool {
    let version_attr = format!("version=\"{version}\"");
    let mut rest = metainfo;

    while let Some(start) = rest.find("<release") {
        let after = &rest[start..];
        let Some(end) = after.find('>') else {
            break;
        };
        let tag = &after[..=end];
        if tag.contains(&version_attr) && has_iso_date_attr(tag) {
            return true;
        }
        rest = &after[end + 1..];
    }

    false
}

fn has_iso_date_attr(tag: &str) -> bool {
    let Some(tail) = tag.split_once("date=\"").map(|(_, t)| t) else {
        return false;
    };
    let Some(date) = tail.split('"').next() else {
        return false;
    };
    let b = date.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[0..4]
            .iter()
            .chain(&b[5..7])
            .chain(&b[8..10])
            .all(u8::is_ascii_digit)
}

fn update_toml_section_version(
    path: &Path,
    section: &str,
    new_version: &str,
    root: &Path,
) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let line_ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let trailing_newline = content.ends_with('\n');
    let mut in_section = false;
    let mut updated = false;
    let mut lines: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == section;
            lines.push(line.to_string());
            continue;
        }
        if in_section
            && !updated
            && let Some(rest) = trimmed.strip_prefix("version")
            && rest.trim_start().starts_with('=')
        {
            let replacement = format!("version = \"{new_version}\"");
            if line != replacement {
                updated = true;
            }
            lines.push(replacement);
            continue;
        }
        lines.push(line.to_string());
    }

    if updated {
        let mut out = lines.join(line_ending);
        if trailing_newline {
            out.push_str(line_ending);
        }
        write_file(path, out)?;
        println!(
            "Updated {}",
            path.strip_prefix(root).unwrap_or(path).display()
        );
    }

    Ok(())
}

fn update_info_plist_version(path: &Path, new_version: &str, root: &Path) -> Result<()> {
    let original = fs::read_to_string(path)?;
    let mut updated = original.clone();
    for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
        updated = replace_plist_string_after_key(&updated, key, new_version);
    }

    if updated != original {
        write_file(path, updated)?;
        println!(
            "Updated {}",
            path.strip_prefix(root).unwrap_or(path).display()
        );
    }

    Ok(())
}

fn replace_plist_string_after_key(plist: &str, key: &str, new_value: &str) -> String {
    let needle = format!("<key>{key}</key>");
    let Some(key_pos) = plist.find(&needle) else {
        return plist.to_string();
    };
    let after_key = &plist[key_pos + needle.len()..];
    let Some(open_rel) = after_key.find("<string>") else {
        return plist.to_string();
    };
    let value_start = key_pos + needle.len() + open_rel + "<string>".len();
    let Some(close_rel) = plist[value_start..].find("</string>") else {
        return plist.to_string();
    };
    let value_end = value_start + close_rel;

    let mut out = String::with_capacity(plist.len() + new_value.len());
    out.push_str(&plist[..value_start]);
    out.push_str(new_value);
    out.push_str(&plist[value_end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "stremio-lightning-xtask-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn validates_semver_strings() {
        assert!(is_valid_semver("0.1.5"));
        assert!(is_valid_semver("1.2.3-rc.1"));
        assert!(!is_valid_semver("0.1"));
        assert!(!is_valid_semver("v0.1.5"));
        assert!(!is_valid_semver("0.1.5-"));
        assert!(!is_valid_semver("0.1.5+meta"));
    }

    #[test]
    fn checks_dated_release_in_metainfo() {
        let xml = r#"<releases><release version="0.1.5" date="2026-07-25"></release></releases>"#;
        assert!(has_dated_metainfo_release(xml, "0.1.5"));
        assert!(!has_dated_metainfo_release(xml, "0.1.6"));
        assert!(!has_dated_metainfo_release(
            r#"<release version="0.1.5" date="2026-7-25">"#,
            "0.1.5"
        ));
    }

    #[test]
    fn bumps_workspace_manifest_and_plist_while_preserving_workspace_inheritance() {
        let root = temp_root("bump");
        write_file(
            root.join("assets/io.github.theguy000.stremio-lightning.metainfo.xml"),
            r#"<component><releases><release version="0.2.0" date="2026-11-13"/></releases></component>"#,
        )
        .unwrap();
        write_file(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/xtask\"]\n\n[workspace.package]\nversion = \"0.1.4\"\n",
        )
        .unwrap();
        write_file(
            root.join("crates/stremio-lightning/Cargo.toml"),
            "[package]\nname = \"stremio-lightning\"\nversion.workspace = true\n\n[dependencies]\nserde = { version = \"0.1.4\" }\n",
        )
        .unwrap();
        write_file(
            root.join("assets/macos/Info.plist"),
            "<dict>\n  <key>CFBundleShortVersionString</key>\n  <string>0.1.4</string>\n  <key>CFBundleVersion</key>\n  <string>0.1.4</string>\n</dict>\n",
        )
        .unwrap();

        bump_version_in(&root, "0.2.0").unwrap();

        let ws_toml = fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(ws_toml.contains("version = \"0.2.0\""));

        let native_toml =
            fs::read_to_string(root.join("crates/stremio-lightning/Cargo.toml")).unwrap();
        assert!(native_toml.contains("version.workspace = true"));
        assert!(native_toml.contains("serde = { version = \"0.1.4\" }"));

        let plist = fs::read_to_string(root.join("assets/macos/Info.plist")).unwrap();
        assert_eq!(plist.matches("<string>0.2.0</string>").count(), 2);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn rejects_missing_metainfo_release() {
        let root = temp_root("missing-release");
        write_file(
            root.join("assets/io.github.theguy000.stremio-lightning.metainfo.xml"),
            r#"<component><releases><release version="0.1.5" date="2026-07-25"/></releases></component>"#,
        )
        .unwrap();

        let err = bump_version_in(&root, "0.9.9").unwrap_err().to_string();
        assert!(err.contains("missing dated AppStream release 0.9.9"));

        fs::remove_dir_all(&root).unwrap();
    }
}
