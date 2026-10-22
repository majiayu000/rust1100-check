//! Output formats: text, JSON, GitHub workflow commands, SARIF 2.1.0.

use std::collections::BTreeMap;
use std::fmt::Write;

use serde_json::json;

use crate::cooldown::Summary;
use crate::{Finding, RULES, Severity};

pub struct Report {
    pub findings: Vec<Finding>,
    pub files_scanned: usize,
    pub cooldown: Option<Summary>,
    /// `rustc --version` of stable and beta when `--beta-diff` ran.
    pub toolchains: Option<(String, String)>,
}

impl Report {
    fn count(&self, s: Severity) -> usize {
        self.findings.iter().filter(|f| f.severity == s).count()
    }
}

pub fn text(r: &Report) -> String {
    let mut out = String::new();
    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in &r.findings {
        by_file.entry(&f.file).or_default().push(f);
    }
    for (file, findings) in by_file {
        let _ = writeln!(out, "{file}");
        for f in findings {
            let _ = writeln!(
                out,
                "  {}:{}  {:<7}  {}\n          {}\n          help: {}",
                f.line,
                f.column,
                f.severity.as_str(),
                f.rule,
                f.message,
                f.help
            );
        }
        out.push('\n');
    }
    if let Some((stable, beta)) = &r.toolchains {
        let _ = writeln!(out, "beta diff: {beta} vs {stable}");
    }
    if let Some(c) = &r.cooldown {
        let _ = writeln!(
            out,
            "cooldown preview (min-publish-age \"{}\"): {} crates.io packages, {} younger than the age, {} would fail to resolve; {} packages from other sources not checked",
            c.age, c.registry_packages, c.young, c.unsatisfiable, c.other_sources
        );
        let _ = writeln!(
            out,
            "to enable it (Cargo 1.100+), add to .cargo/config.toml:\n\n[registry]\nglobal-min-publish-age = \"{}\"\n",
            c.age
        );
    }
    let _ = writeln!(
        out,
        "files checked: {}; errors: {}, warnings: {}, notes: {}",
        r.files_scanned,
        r.count(Severity::Error),
        r.count(Severity::Warning),
        r.count(Severity::Note)
    );
    out
}

pub fn json(r: &Report) -> String {
    let value = json!({
        "tool": "rust1100-check",
        "version": env!("CARGO_PKG_VERSION"),
        "files_scanned": r.files_scanned,
        "findings": r.findings,
        "summary": {
            "error": r.count(Severity::Error),
            "warning": r.count(Severity::Warning),
            "note": r.count(Severity::Note),
        },
        "cooldown": r.cooldown.as_ref().map(|c| json!({
            "min_publish_age": c.age,
            "registry_packages": c.registry_packages,
            "other_sources": c.other_sources,
            "young": c.young,
            "unsatisfiable": c.unsatisfiable,
        })),
        "toolchains": r.toolchains.as_ref().map(|(s, b)| json!({"stable": s, "beta": b})),
    });
    serde_json::to_string_pretty(&value).expect("report serializes")
}

/// Escape per GitHub's workflow-command rules.
fn gh_escape(s: &str, property: bool) -> String {
    let s = s
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    if property {
        s.replace(':', "%3A").replace(',', "%2C")
    } else {
        s
    }
}

pub fn github(r: &Report) -> String {
    let mut out = String::new();
    for f in &r.findings {
        let level = match f.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "notice",
        };
        let _ = writeln!(
            out,
            "::{level} file={},line={},col={},title={}::{}",
            gh_escape(&f.file, true),
            f.line,
            f.column,
            gh_escape(&format!("rust1100-check/{}", f.rule), true),
            gh_escape(&format!("{}\nhelp: {}", f.message, f.help), false)
        );
    }
    out
}

pub fn sarif(r: &Report) -> String {
    let rules: Vec<_> = RULES
        .iter()
        .map(|rule| json!({"id": rule.id, "shortDescription": {"text": rule.summary}}))
        .collect();
    let results: Vec<_> = r
        .findings
        .iter()
        .map(|f| {
            json!({
                "ruleId": f.rule,
                "level": f.severity.as_str(),
                "message": {"text": format!("{}\nhelp: {}", f.message, f.help)},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": f.file},
                    "region": {"startLine": f.line, "startColumn": f.column},
                }}],
            })
        })
        .collect();
    let value = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {
                "name": "rust1100-check",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/majiayu000/rust1100-check",
                "rules": rules,
            }},
            "results": results,
        }],
    });
    serde_json::to_string_pretty(&value).expect("report serializes")
}
