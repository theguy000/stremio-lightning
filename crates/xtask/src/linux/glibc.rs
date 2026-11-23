use crate::common::{Result, program_exists};
use crate::{LINUX_FLATPAK_RUNTIME, LINUX_FLATPAK_RUNTIME_VERSION};
use std::{collections::BTreeSet, env, fmt, fs, io::Read, path::Path, process::Command};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct GlibcVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl GlibcVersion {
    const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for GlibcVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.patch == 0 {
            write!(f, "GLIBC_{}.{}", self.major, self.minor)
        } else {
            write!(f, "GLIBC_{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FlatpakGlibcPolicy {
    max_supported: GlibcVersion,
}

fn current_flatpak_glibc_policy() -> Result<FlatpakGlibcPolicy> {
    flatpak_glibc_policy(LINUX_FLATPAK_RUNTIME, LINUX_FLATPAK_RUNTIME_VERSION).ok_or_else(|| {
        format!(
            "missing GLIBC compatibility policy for Flatpak runtime {LINUX_FLATPAK_RUNTIME}//{LINUX_FLATPAK_RUNTIME_VERSION}; update xtask before packaging"
        )
        .into()
    })
}

fn flatpak_glibc_policy(runtime: &str, runtime_version: &str) -> Option<FlatpakGlibcPolicy> {
    match (runtime, runtime_version) {
        ("org.gnome.Platform", "50") | ("org.freedesktop.Platform", "25.08") => {
            Some(FlatpakGlibcPolicy {
                max_supported: GlibcVersion::new(2, 42, 0),
            })
        }
        _ => None,
    }
}

fn glibc_symbols_newer_than(
    readelf_version_info: &str,
    policy: FlatpakGlibcPolicy,
) -> Vec<GlibcVersion> {
    let mut symbols = BTreeSet::new();
    let mut remaining = readelf_version_info;

    while let Some(index) = remaining.find("GLIBC_") {
        let symbol_tail = &remaining[index + "GLIBC_".len()..];
        let raw_version = symbol_tail
            .split(|character: char| !(character.is_ascii_digit() || character == '.'))
            .next()
            .unwrap_or_default();

        if let Some(version) = parse_glibc_version(raw_version)
            && version > policy.max_supported
        {
            symbols.insert(version);
        }

        let consumed = raw_version.len().max(1).min(symbol_tail.len());
        remaining = &symbol_tail[consumed..];
    }

    symbols.into_iter().collect()
}

fn parse_glibc_version(raw: &str) -> Option<GlibcVersion> {
    let mut parts = raw.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    if parts.next().is_some() {
        return None;
    }
    Some(GlibcVersion::new(major, minor, patch))
}

pub(super) fn validate_flatpak_glibc_symbols(payload_dir: &Path) -> Result<()> {
    if !program_exists("readelf") {
        return Err("missing readelf. Install binutils before packaging the Flatpak.".into());
    }

    let files_dir = payload_dir.join("files");
    if !files_dir.is_dir() {
        return Err(format!(
            "Flatpak payload is missing files directory for GLIBC validation: {}",
            files_dir.display()
        )
        .into());
    }

    let policy = current_flatpak_glibc_policy()?;
    let mut offenders = Vec::new();
    collect_flatpak_glibc_symbol_offenders(&files_dir, policy, &mut offenders)?;

    if offenders.is_empty() {
        return Ok(());
    }

    if env::var("IGNORE_GLIBC").is_ok() || env::var("STREMIO_LIGHTNING_IGNORE_GLIBC").is_ok() {
        println!("WARNING: Ignoring GLIBC symbol compatibility error as requested.");
        return Ok(());
    }

    Err(format!(
        "Flatpak payload contains libraries requiring GLIBC newer than {LINUX_FLATPAK_RUNTIME}//{LINUX_FLATPAK_RUNTIME_VERSION} (maximum supported symbol: {}):\n       {}\n       Build the Linux payload with a distro/SDK whose GLIBC is compatible with the target Flatpak runtime.",
        policy.max_supported,
        offenders.join("\n       ")
    )
    .into())
}

fn collect_flatpak_glibc_symbol_offenders(
    path: &Path,
    policy: FlatpakGlibcPolicy,
    offenders: &mut Vec<String>,
) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_flatpak_glibc_symbol_offenders(&entry?.path(), policy, offenders)?;
        }
        return Ok(());
    }

    if !path.is_file() || !is_elf_file(path)? {
        return Ok(());
    }

    let output = Command::new("readelf")
        .arg("--version-info")
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let newer_symbols = glibc_symbols_newer_than(&stdout, policy);
    if !newer_symbols.is_empty() {
        let symbols = newer_symbols
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        offenders.push(format!("{} requires {symbols}", path.display()));
    }

    Ok(())
}

/// Inspects only the 4-byte ELF magic header so large binaries are never buffered into memory.
fn is_elf_file(path: &Path) -> Result<bool> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0_u8; 4];
    let read = file.read(&mut magic)?;
    Ok(read == 4 && &magic == b"\x7fELF")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_flatpak_runtime_has_glibc_policy() {
        let policy = flatpak_glibc_policy(LINUX_FLATPAK_RUNTIME, LINUX_FLATPAK_RUNTIME_VERSION)
            .expect("current Flatpak runtime must have a GLIBC policy");
        assert_eq!(policy.max_supported, GlibcVersion::new(2, 42, 0));
    }

    #[test]
    fn glibc_policy_reports_symbols_newer_than_runtime_limit() {
        let policy = FlatpakGlibcPolicy {
            max_supported: GlibcVersion::new(2, 42, 0),
        };
        let symbols = glibc_symbols_newer_than(
            "Name: GLIBC_2.34\n0x00 0x00 4 (GLIBC_2.43)\nName: GLIBC_2.2.5\nName: GLIBC_2.44\n",
            policy,
        );
        assert_eq!(
            symbols,
            vec![GlibcVersion::new(2, 43, 0), GlibcVersion::new(2, 44, 0)]
        );
    }
}
