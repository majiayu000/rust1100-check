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
        match kind {
            FileKind::CiYaml => yaml::check(&rel, &text, &mut findings),
            FileKind::Script => script::check(&rel, &text, 0, &mut findings),
        }
    }
    findings.sort_by(|a, b| (&a.file, a.line, a.column).cmp(&(&b.file, b.line, b.column)));
    Ok((findings, files))
}
