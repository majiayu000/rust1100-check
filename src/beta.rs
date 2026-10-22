//! R6: diagnostics that `cargo +beta check` (1.100 until Nov 12) emits and `cargo +stable check` does not.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::{Finding, Severity};

pub const NEW_DIAGNOSTIC: &str = "beta-new-diagnostic";
const RELEASE_NOTES: &str = "https://github.com/rust-lang/rust/issues/163811";

/// (level, code, file, line, column, message)
pub type Diag = (String, String, String, usize, usize, String);

#[derive(Deserialize)]
struct CargoLine {
    reason: String,
    message: Option<CompilerMessage>,
}

#[derive(Deserialize)]
struct CompilerMessage {
    level: String,
    message: String,
    code: Option<Code>,
    spans: Vec<Span>,
}

#[derive(Deserialize)]
struct Code {
    code: String,
}

#[derive(Deserialize)]
struct Span {
    file_name: String,
    line_start: usize,
    column_start: usize,
    is_primary: bool,
}

static CARGO_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(warning|error)(?:\[([\w:-]+)\])?: (.*)$").unwrap());
static CARGO_LOCATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-->\s+(\S+?):(\d+)(?::(\d+))?\s*$").unwrap());
/// Summary lines whose text changes with counts, not new problems.
static NOISE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"generated \d+ warning|could not compile|build failed|aborting due to").unwrap()
});

/// Parse `cargo check --message-format=json` stdout plus cargo's own stderr diagnostics.
pub fn parse_output(stdout: &str, stderr: &str) -> BTreeSet<Diag> {
    let mut diags = BTreeSet::new();
    for line in stdout.lines() {
        let Ok(parsed) = serde_json::from_str::<CargoLine>(line) else {
            continue;
        };
        let Some(msg) = parsed
            .message
            .filter(|_| parsed.reason == "compiler-message")
        else {
            continue;
        };
        if !matches!(msg.level.as_str(), "error" | "warning") || NOISE.is_match(&msg.message) {
            continue;
        }
        let Some(span) = msg.spans.iter().find(|s| s.is_primary) else {
            continue;
        };
        diags.insert((
            msg.level,
            msg.code.map(|c| c.code).unwrap_or_default(),
            span.file_name.clone(),
            span.line_start,
            span.column_start,
            msg.message,
        ));
    }
    let lines: Vec<&str> = stderr.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let Some(c) = CARGO_HEADER.captures(line) else {
            continue;
        };
        if NOISE.is_match(&c[3]) {
            continue;
        }
        let location = lines[i + 1..]
            .iter()
            .take_while(|l| !CARGO_HEADER.is_match(l))
            .find_map(|l| CARGO_LOCATION.captures(l));
        let (file, line_no, col) = match location {
            Some(loc) => (
                loc[1].to_owned(),
                loc[2].parse().unwrap_or(1),
                loc.get(3)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(1),
            ),
            None => ("Cargo.toml".to_owned(), 1, 1),
        };
        diags.insert((
            c[1].to_owned(),
            c.get(2).map(|m| m.as_str().to_owned()).unwrap_or_default(),
            file,
            line_no,
            col,
            c[3].to_owned(),
        ));
    }
    diags
}

fn toolchain_version(toolchain: &str) -> Result<String, String> {
    let out = Command::new("rustup")
        .args(["run", toolchain, "rustc", "--version"])
        .env_remove("RUSTUP_TOOLCHAIN")
        .output()
        .map_err(|e| format!("running rustup: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "toolchain `{toolchain}` is not usable ({}); install it with `rustup toolchain install {toolchain} --profile minimal`",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn check_with(root: &Path, toolchain: &str) -> Result<BTreeSet<Diag>, String> {
    let target_dir = root.join("target").join("rust1100-check").join(toolchain);
    let out = Command::new("rustup")
        .args([
            "run",
            toolchain,
            "cargo",
            "check",
            "--workspace",
            "--all-targets",
            "--message-format=json",
        ])
        .arg("--target-dir")
        .arg(&target_dir)
        .current_dir(root)
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("CARGO")
        .env_remove("RUSTC")
        .output()
        .map_err(|e| format!("running cargo +{toolchain} check: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // A failed build still prints JSON diagnostics; only a run with none at all is a tool failure.
    if !out.status.success()
        && !stdout
            .lines()
            .any(|l| l.contains("\"reason\":\"compiler-message\""))
    {
        return Err(format!(
            "cargo +{toolchain} check failed:\n{}",
            stderr.trim()
        ));
    }
    Ok(parse_output(&stdout, &stderr))
}

/// Diagnostics present only with beta. Returns findings and the two `rustc --version` strings.
pub fn run(root: &Path, baseline: &str) -> Result<(Vec<Finding>, String, String), String> {
    let stable_version = toolchain_version(baseline)?;
    let beta_version = toolchain_version("beta")?;
    let stable = check_with(root, baseline)?;
    let beta = check_with(root, "beta")?;
    Ok((
        diff(&stable, &beta, &beta_version),
        stable_version,
        beta_version,
    ))
}

pub fn diff(stable: &BTreeSet<Diag>, beta: &BTreeSet<Diag>, beta_version: &str) -> Vec<Finding> {
    beta.difference(stable)
        .map(|(level, code, file, line, column, message)| Finding {
            rule: NEW_DIAGNOSTIC,
            severity: if level == "error" { Severity::Error } else { Severity::Warning },
            file: file.clone(),
            line: *line,
            column: *column,
            message: if code.is_empty() {
                format!("new with {beta_version}: {message}")
            } else {
                format!("new with {beta_version}: {message} [{code}]")
            },
            help: format!("not reported by the baseline toolchain; see the 1.100 release notes: {RELEASE_NOTES}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compiler_json_and_cargo_stderr() {
        let stdout = r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused variable: `x`","code":{"code":"unused_variables"},"spans":[{"file_name":"src/lib.rs","line_start":3,"column_start":9,"is_primary":true}]}}
{"reason":"compiler-message","message":{"level":"warning","message":"`a` (lib) generated 1 warning","code":null,"spans":[]}}
{"reason":"build-finished","success":true}"#;
        let stderr = "warning: unused dependency\n --> Cargo.toml:9:1\n  |\nwarning: `a` (lib) generated 1 warning\n";
        let diags = parse_output(stdout, stderr);
        assert_eq!(diags.len(), 2);
        assert!(
            diags
                .iter()
                .any(|d| d.2 == "Cargo.toml" && d.3 == 9 && d.5 == "unused dependency")
        );
    }

    #[test]
    fn diff_reports_beta_only() {
        let d = |m: &str| {
            (
                "warning".to_owned(),
                String::new(),
                "src/lib.rs".to_owned(),
                1,
                1,
                m.to_owned(),
            )
        };
        let stable = BTreeSet::from([d("old")]);
        let beta = BTreeSet::from([d("old"), d("new")]);
        let found = diff(&stable, &beta, "rustc 1.100.0-beta.7");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("new"));
    }
}
