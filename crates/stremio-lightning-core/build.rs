use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=STREMIO_LIGHTNING_VERSION");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_TYPE");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");
    println!("cargo:rerun-if-changed=../../.git/refs/tags");

    let version = env::var("STREMIO_LIGHTNING_VERSION")
        .ok()
        .and_then(normalize_version)
        .or_else(github_tag_version)
        .or_else(git_tag_version)
        .unwrap_or_else(|| "0.0.0".to_string());

    println!("cargo:rustc-env=STREMIO_LIGHTNING_VERSION={version}");
}

fn github_tag_version() -> Option<String> {
    (env::var("GITHUB_REF_TYPE").ok().as_deref() == Some("tag"))
        .then(|| env::var("GITHUB_REF_NAME").ok().and_then(normalize_version))
        .flatten()
}

fn git_tag_version() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--match", "v[0-9]*", "--abbrev=0"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| {
            String::from_utf8(output.stdout)
                .ok()
                .and_then(normalize_version)
        })
        .flatten()
}

fn normalize_version(version: String) -> Option<String> {
    let version = version.trim().trim_start_matches('v');
    (!version.is_empty()
        && version.starts_with(|character: char| character.is_ascii_digit())
        && version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '+' | '-')
        }))
    .then(|| version.to_string())
}
