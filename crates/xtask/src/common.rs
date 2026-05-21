use std::{
    env,
    error::Error,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask must live under crates/xtask")
        .to_path_buf()
}

pub fn package_version() -> Result<String> {
    let raw = env::var("STREMIO_LIGHTNING_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            if env::var("GITHUB_REF_TYPE").ok().as_deref() == Some("tag") {
                env::var("GITHUB_REF_NAME").ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    let version = raw.trim().trim_start_matches('v').to_string();
    if version.is_empty() {
        return Err("package version is empty".into());
    }
    if !version.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '+' | '~' | '-' | ':')
    }) {
        return Err(format!("package version contains unsupported characters: {raw}").into());
    }
    if !version
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        return Err(format!(
            "package version must start with a digit after optional 'v' prefix: {raw}"
        )
        .into());
    }

    Ok(version)
}

pub fn require_program(program: &str, setup_hint: &str) -> Result<()> {
    if program_exists(program) {
        return Ok(());
    }

    Err(format!("missing required command: {program}\n       {setup_hint}").into())
}

pub fn program_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub fn chmod_executable(path: impl AsRef<Path>) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = path.as_ref();
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub fn copy_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to).map_err(|error| {
        format!(
            "failed to copy {} to {}: {error}",
            from.display(),
            to.display()
        )
    })?;
    Ok(())
}

pub fn copy_dir_recursive(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    fs::create_dir_all(to)?;

    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(source, destination)?;
        } else if source.is_file() {
            copy_file(source, destination)?;
        }
    }

    Ok(())
}

pub fn inno_path(path: &Path) -> String {
    path.to_string_lossy().replace('"', "\"\"")
}

pub fn write_file(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

pub fn remove_dir_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn run_npm(args: &[&str]) -> Result<()> {
    let program = if cfg!(windows) { "npm.cmd" } else { "npm" };
    run_program(program, args)
}

pub fn bash_program() -> OsString {
    #[cfg(windows)]
    {
        let git_bash = Path::new(r"C:\Program Files\Git\bin\bash.exe");
        if git_bash.is_file() {
            return git_bash.as_os_str().to_os_string();
        }
    }

    OsString::from("bash")
}

pub fn run_program<I, S>(program: impl AsRef<OsStr>, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    run_program_in(root(), program, args)
}

pub fn run_program_in<I, S>(
    cwd: impl AsRef<Path>,
    program: impl AsRef<OsStr>,
    args: I,
) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut command = Command::new(program);
    command.args(args.into_iter().map(Into::into));
    run_command_in(&mut command, cwd)
}

pub fn run_command(command: &mut Command) -> Result<()> {
    run_command_in(command, root())
}

pub fn run_command_in(command: &mut Command, cwd: impl AsRef<Path>) -> Result<()> {
    let cwd = cwd.as_ref();
    command
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = command.status().map_err(|error| {
        format!(
            "failed to start command in {}: {command:?}\n       Cause: {error}",
            cwd.display()
        )
    })?;
    if !status.success() {
        return Err(format!("command failed with status {status:?}: {command:?}").into());
    }
    Ok(())
}

pub fn required_file(path: &Path, setup_hint: &str) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|_| {
        format!(
            "missing required file: {}\n       Run: {setup_hint}",
            path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "required file is empty or invalid: {}\n       Run: {setup_hint}",
            path.display()
        )
        .into());
    }
    Ok(())
}

pub fn required_executable_file(path: impl AsRef<Path>, setup_hint: &str) -> Result<()> {
    let path = path.as_ref();
    required_file(path, setup_hint)?;
    if !is_executable_file(path) {
        return Err(format!(
            "required file is not executable: {}\n       Run: {setup_hint}",
            path.display()
        )
        .into());
    }
    Ok(())
}
