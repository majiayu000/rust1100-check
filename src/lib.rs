//! Find where a repository's CI, scripts and dependencies break on Rust 1.100.

pub mod beta;
pub mod cooldown;
pub mod report;
pub mod script;
pub mod yaml;

use std::path::Path;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Note,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    /// Path relative to the scanned root, with `/` separators.
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub help: String,
}

pub struct Rule {
    pub id: &'static str,
    pub summary: &'static str,
}

pub const RULES: &[Rule] = &[
    Rule {
        id: yaml::TRUNCATED,
        summary: "Unquoted YAML version ending in 0 is read as a different number (1.90 -> 1.9, 1.100 -> 1.1)",
    },
    Rule {
        id: yaml::FRAGILE,
        summary: "Unquoted two-part Rust version in YAML becomes 1.1 when bumped to 1.100",
    },
    Rule {
        id: yaml::UNPARSABLE,
        summary: "CI YAML file could not be parsed, so it was not checked",
    },
    Rule {
        id: script::STRING_COMPARE,
        summary: "Rust version compared as a string or float, or cut to a fixed width",
    },
    Rule {
        id: script::TWO_DIGIT,
        summary: "Pattern only matches a two-digit minor version",
    },
    Rule {
        id: cooldown::UNSATISFIABLE,
        summary: "With min-publish-age, no old-enough version satisfies a requirement",
    },
    Rule {
        id: cooldown::YOUNG,
        summary: "Locked version is younger than min-publish-age; a fresh resolve would pick an older one",
    },
    Rule {
        id: beta::NEW_DIAGNOSTIC,
        summary: "Diagnostic emitted by beta (1.100) but not by stable",
    },
];

/// A finding is dropped when this text appears on its line or the line above.
pub const IGNORE_MARKER: &str = "rust1100-check: ignore";

/// Quote every unquoted version reported by the YAML rules, in place. Returns how many
/// values were quoted.
pub fn fix_yaml(root: &Path, findings: &[Finding]) -> Result<usize, String> {
    let mut by_file: std::collections::BTreeMap<&str, Vec<(usize, usize)>> = Default::default();
    for f in findings
        .iter()
        .filter(|f| f.rule == yaml::TRUNCATED || f.rule == yaml::FRAGILE)
    {
        by_file.entry(&f.file).or_default().push((f.line, f.column));
    }
    let mut total = 0;
    for (file, positions) in by_file {
        let path = root.join(file);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        let (fixed, n) = yaml::quote_versions(&text, &positions);
        if n > 0 {
            std::fs::write(&path, fixed).map_err(|e| format!("writing {}: {e}", path.display()))?;
            total += n;
        }
    }
    Ok(total)
}

pub enum FileKind {
    CiYaml,
    Script,
}

/// Decide whether a repository-relative path is something we check.
pub fn classify(rel: &str) -> Option<FileKind> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let is_yaml = name.ends_with(".yml") || name.ends_with(".yaml");
    if is_yaml {
        let stem = name.trim_end_matches(".yml").trim_end_matches(".yaml");
        let ci = rel.starts_with(".github/")
            || stem == "action"
            || rel == ".gitlab-ci.yml"
            || rel == ".circleci/config.yml"
            || rel == ".travis.yml"
            || rel == "azure-pipelines.yml"
            || rel == ".cirrus.yml"
            || rel == "bitbucket-pipelines.yml"
            || stem == "compose"
            || stem.starts_with("docker-compose");
        return ci.then_some(FileKind::CiYaml);
    }
    let script = name == "Makefile"
        || name == "justfile"
        || name == "build.rs"
        || [".sh", ".bash", ".mk", ".py", ".js", ".mjs", ".ts"]
            .iter()
            .any(|ext| name.ends_with(ext));
    script.then_some(FileKind::Script)
}

/// Run the offline checks (YAML and scripts). Returns findings and the number of files checked.
pub fn scan_static(root: &Path) -> Result<(Vec<Finding>, usize), String> {
    let mut findings = Vec::new();
    let mut files = 0;
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| {
            !matches!(
                e.file_name().to_str(),
                Some(".git" | "target" | "node_modules")
            )
        })
        .build();
    for entry in walker {
        let entry = entry.map_err(|e| format!("walking {}: {e}", root.display()))?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        let Some(kind) = classify(&rel) else { continue };
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            // Not UTF-8 (binary or another encoding): nothing we can match against.
            continue;
        };
        files += 1;
        let mut found = Vec::new();
        match kind {
            FileKind::CiYaml => yaml::check(&rel, &text, &mut found),
            FileKind::Script => script::check(&rel, &text, 0, &mut found),
        }
        let lines: Vec<&str> = text.lines().collect();
        let ignored = |line: usize| {
            [line.checked_sub(1), line.checked_sub(2)]
                .into_iter()
                .flatten()
                .any(|i| lines.get(i).is_some_and(|l| l.contains(IGNORE_MARKER)))
        };
        findings.extend(found.into_iter().filter(|f| !ignored(f.line)));
    }
    findings.sort_by(|a, b| (&a.file, a.line, a.column).cmp(&(&b.file, b.line, b.column)));
    Ok((findings, files))
}
