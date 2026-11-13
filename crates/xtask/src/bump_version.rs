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

    let old_version = read_package_version(&root.join("crates/xtask/Cargo.toml"));
    println!("Bumping version to {new_version}...");

    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        let mut cargo_files: Vec<_> = fs::read_dir(&crates_dir)?
            .filter_map(|entry| entry.ok().map(|e| e.path().join("Cargo.toml")))
            .filter(|path| path.is_file())
            .collect();
        cargo_files.sort();
        for cargo_toml in cargo_files {
            update_cargo_toml_version(&cargo_toml, new_version, root)?;
        }
    }

    let info_plist = root.join("assets/macos/Info.plist");
    if info_plist.is_file() {
        update_info_plist_version(&info_plist, new_version, root)?;
    }

    if let Some(old_version) = old_version.as_deref()
        && old_version != new_version
    {
        let app_update = root.join("crates/stremio-lightning-core/src/app_update.rs");
        if app_update.is_file() {
            update_app_update_version(&app_update, old_version, new_version, root)?;
        }
    }

    Ok(())
}

pub fn is_valid_semver(version: &str) -> bool {
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };

    let mut parts = core.split('.');
    let valid_core = matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(a), Some(b), Some(c), None)
            if !a.is_empty()
                && !b.is_empty()
                && !c.is_empty()
                && a.chars().all(|ch| ch.is_ascii_digit())
                && b.chars().all(|ch| ch.is_ascii_digit())
                && c.chars().all(|ch| ch.is_ascii_digit())
    );
    if !valid_core {
        return false;
    }

    match pre {
        Some(pre) => {
            !pre.is_empty()
                && pre
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
        }
        None => true,
    }
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
    let Some(pos) = tag.find("date=\"") else {
        return false;
    };
    let tail = &tag[pos + "date=\"".len()..];
    let Some(end) = tail.find('"') else {
        return false;
    };
    let date = &tail[..end];
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

fn read_package_version(cargo_toml: &Path) -> Option<String> {
    let content = fs::read_to_string(cargo_toml).ok()?;
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package
            && let Some(rest) = trimmed.strip_prefix("version")
            && let Some(rest) = rest.trim_start().strip_prefix('=')
        {
            let value = rest.trim();
            if let Some(stripped) = value.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                return Some(stripped.to_string());
            }
        }
    }
    None
}

fn update_cargo_toml_version(path: &Path, new_version: &str, root: &Path) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let line_ending = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let trailing_newline = content.ends_with('\n');
    let mut in_package = false;
    let mut updated = false;
    let mut lines: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            lines.push(line.to_string());
            continue;
        }
        if in_package
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

fn update_app_update_version(
    path: &Path,
    old_version: &str,
    new_version: &str,
    root: &Path,
) -> Result<()> {
    let original = fs::read_to_string(path)?;
    let updated = original
        .replace(
            &format!("current_version: \"{old_version}\".to_string()"),
            &format!("current_version: \"{new_version}\".to_string()"),
        )
        .replace(
            &format!("\"currentVersion\": \"{old_version}\""),
            &format!("\"currentVersion\": \"{new_version}\""),
        );

    if updated != original {
        write_file(path, updated)?;
        println!(
            "Updated {}",
            path.strip_prefix(root).unwrap_or(path).display()
        );
    }

    Ok(())
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
    fn bumps_crate_manifests_plist_and_app_update() {
        let root = temp_root("bump");
        write_file(
            root.join("assets/io.github.theguy000.stremio-lightning.metainfo.xml"),
            r#"<component><releases><release version="0.2.0" date="2026-11-13"/></releases></component>"#,
        )
        .unwrap();
        write_file(
            root.join("crates/xtask/Cargo.toml"),
            "[package]\nname = \"xtask\"\nversion = \"0.1.4\"\n",
        )
        .unwrap();
        write_file(
            root.join("crates/stremio-lightning/Cargo.toml"),
            "[package]\nname = \"stremio-lightning\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = { version = \"0.1.4\" }\n",
        )
        .unwrap();
        write_file(
            root.join("assets/macos/Info.plist"),
            "<dict>\n  <key>CFBundleShortVersionString</key>\n  <string>0.1.4</string>\n  <key>CFBundleVersion</key>\n  <string>0.1.4</string>\n</dict>\n",
        )
        .unwrap();
        write_file(
            root.join("crates/stremio-lightning-core/src/app_update.rs"),
            "current_version: \"0.1.4\".to_string()\n\"currentVersion\": \"0.1.4\"\ncheck_app_update(\"0.0.0\")\n",
        )
        .unwrap();

        bump_version_in(&root, "0.2.0").unwrap();

        let native_toml =
            fs::read_to_string(root.join("crates/stremio-lightning/Cargo.toml")).unwrap();
        assert!(native_toml.contains("version = \"0.2.0\""));
        assert!(native_toml.contains("serde = { version = \"0.1.4\" }"));

        let plist = fs::read_to_string(root.join("assets/macos/Info.plist")).unwrap();
        assert_eq!(plist.matches("<string>0.2.0</string>").count(), 2);

        let app_update =
            fs::read_to_string(root.join("crates/stremio-lightning-core/src/app_update.rs"))
                .unwrap();
        assert!(app_update.contains("current_version: \"0.2.0\".to_string()"));
        assert!(app_update.contains("\"currentVersion\": \"0.2.0\""));
        assert!(app_update.contains("check_app_update(\"0.0.0\")"));

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
