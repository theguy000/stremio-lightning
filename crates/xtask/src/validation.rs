use crate::common::{Result, root};
use std::{
    ffi::{OsStr, OsString},
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    thread,
};

pub fn run_validation() -> Result<()> {
    let mut warnings = Vec::new();

    println!("==> [1/5] Checking Rust formatting...");
    let w1 = run_validation_step("cargo", ["fmt", "--all", "--", "--check"])?;
    if w1 > 0 {
        warnings.push(("Checking Rust formatting", w1));
    }

    println!("==> [2/5] Running Rust clippy lints...");
    let w2 = run_validation_step("cargo", ["clippy", "--workspace", "--all-targets"])?;
    if w2 > 0 {
        warnings.push(("Running Rust clippy lints", w2));
    }

    println!("==> [3/5] Running Rust unit/integration tests...");
    let w3 = run_validation_step("cargo", ["test", "--workspace"])?;
    if w3 > 0 {
        warnings.push(("Running Rust unit/integration tests", w3));
    }

    println!("==> [4/5] Running frontend tests...");
    let w4 = run_validation_step_npm(&["run", "test:ui"])?;
    if w4 > 0 {
        warnings.push(("Running frontend tests", w4));
    }

    println!("==> [5/5] Building frontend UI bundle...");
    let w5 = run_validation_step_npm(&["run", "build:ui"])?;
    if w5 > 0 {
        warnings.push(("Building frontend UI bundle", w5));
    }

    if warnings.is_empty() {
        println!("==> All validations completed successfully!");
    } else {
        println!("==> All validations completed successfully, but warnings were detected:");
        for (step, count) in warnings {
            println!(
                "    • {step}: {count} warning{}",
                if count == 1 { "" } else { "s" }
            );
        }
    }
    Ok(())
}

fn run_validation_step<I, S>(program: impl AsRef<OsStr>, args: I) -> Result<usize>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut command = Command::new(program);
    command.args(args.into_iter().map(Into::into));
    run_command_capturing_warnings(&mut command, root())
}

fn run_validation_step_npm(args: &[&str]) -> Result<usize> {
    let program = if cfg!(windows) { "npm.cmd" } else { "npm" };
    run_validation_step(program, args)
}

fn run_command_capturing_warnings(
    command: &mut Command,
    cwd: impl AsRef<std::path::Path>,
) -> Result<usize> {
    let cwd = cwd.as_ref();
    command
        .current_dir(cwd)
        .env("FORCE_COLOR", "1")
        .env("CARGO_TERM_COLOR", "always")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to start command in {}: {command:?}\n       Cause: {error}",
            cwd.display()
        )
    })?;

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let stderr = child.stderr.take().expect("Failed to open stderr");

    let stdout_handle = thread::spawn(move || {
        let mut count = 0;
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(|line| line.ok()) {
            println!("{}", line);
            if is_warning_line(&line) {
                count += 1;
            }
        }
        count
    });

    let stderr_handle = thread::spawn(move || {
        let mut count = 0;
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(|line| line.ok()) {
            eprintln!("{}", line);
            if is_warning_line(&line) {
                count += 1;
            }
        }
        count
    });

    let status = child.wait().map_err(|error| {
        format!("failed to wait for command to complete: {command:?}\n       Cause: {error}")
    })?;

    let stdout_count = stdout_handle.join().unwrap_or(0);
    let stderr_count = stderr_handle.join().unwrap_or(0);

    if !status.success() {
        return Err(format!("command failed with status {status:?}: {command:?}").into());
    }

    Ok(stdout_count + stderr_count)
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn is_warning_line(line: &str) -> bool {
    let stripped = strip_ansi(line);
    let clean = stripped.trim();
    if clean.is_empty() {
        return false;
    }
    let lower = clean.to_lowercase();

    // Rust: "warning: ..." or "warning[E0123]: ..."
    if lower.contains("warning:") || lower.contains("warning[") {
        return true;
    }

    // Vite/Webpack/Rollup: starts with "warn" or "warning" followed by space or punctuation
    // e.g. "WARN  inlineDynamicImports option is ignored"
    if lower.starts_with("warn ")
        || lower.starts_with("warning ")
        || lower.starts_with("warn:")
        || lower.starts_with("warning:")
    {
        return true;
    }

    // Common prefixes or tags in logs
    if lower.contains("[warn]") || lower.contains("[warning]") || lower.contains("console.warn") {
        return true;
    }

    // Check for uppercase word matches specifically in the original line
    if clean.starts_with("WARN") || clean.starts_with("WARNING") {
        return true;
    }

    false
}
