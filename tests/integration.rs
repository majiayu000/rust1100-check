use std::collections::HashMap;
use std::path::Path;

use rust1100_check::{Severity, cooldown, scan_static};

fn scan(fixture: &str) -> Vec<(String, usize, &'static str, Severity)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let (findings, _) = scan_static(&root).unwrap();
    findings
        .into_iter()
        .map(|f| (f.file, f.line, f.rule, f.severity))
        .collect()
}

fn hit(
    file: &str,
    line: usize,
    rule: &'static str,
    severity: Severity,
) -> (String, usize, &'static str, Severity) {
    (file.to_owned(), line, rule, severity)
}

const TRUNC: &str = "yaml-float-version-truncated";
const FRAGILE: &str = "yaml-float-rust-version";
const CMP: &str = "version-compare-as-string";
const TWO: &str = "two-digit-minor-pattern";

#[test]
fn actions_rs_matrix_is_fragile() {
    let f = ".github/workflows/CI.yml";
    let mut expected = vec![hit(f, 10, FRAGILE, Severity::Warning)];
    expected.extend((0..4).map(|_| hit(f, 16, FRAGILE, Severity::Warning)));
    assert_eq!(scan("actions-rs-matrix"), expected);
}

#[test]
fn actions_rs_trailing_zero_is_an_error_today() {
    assert_eq!(
        scan("actions-rs-trailing-zero"),
        vec![hit(".github/workflows/ci.yml", 11, TRUNC, Severity::Error)]
    );
}

#[test]
fn dtolnay_matrix_is_suppressed() {
    assert_eq!(scan("dtolnay-matrix"), vec![]);
}

#[test]
fn dtolnay_trailing_zero_is_a_note() {
    assert_eq!(
        scan("dtolnay-trailing-zero"),
        vec![hit(
            ".github/workflows/rust-clippy.yml",
            10,
            TRUNC,
            Severity::Note
        )]
    );
}

#[test]
fn env_msrv_is_fragile() {
    assert_eq!(
        scan("env-msrv"),
        vec![hit(
            ".github/workflows/ci.yml",
            5,
            FRAGILE,
            Severity::Warning
        )]
    );
}

#[test]
fn gitlab_variables() {
    assert_eq!(
        scan("gitlab"),
        vec![
            hit(".gitlab-ci.yml", 2, TRUNC, Severity::Error),
            hit(".gitlab-ci.yml", 3, TRUNC, Severity::Warning),
        ]
    );
}

#[test]
fn quoted_and_three_part_versions_are_clean() {
    assert_eq!(scan("quoted-clean"), vec![]);
}

#[test]
fn scripts() {
    assert_eq!(
        scan("scripts"),
        vec![
            hit("build.sh", 4, CMP, Severity::Warning),
            hit("scripts/versions.sh", 3, TWO, Severity::Warning),
        ]
    );
}

#[test]
fn cooldown_preview() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cooldown");
    let lock = std::fs::read_to_string(dir.join("Cargo.lock")).unwrap();
    let index: HashMap<String, String> = ["young-dep", "fresh-only", "old-dep", "chain-child"]
        .iter()
        .map(|n| {
            (
                n.to_string(),
                std::fs::read_to_string(dir.join("index").join(n)).unwrap(),
            )
        })
        .collect();
    let reqs = vec![
        cooldown::Requirement {
            crate_name: "young-dep".into(),
            req: "^1.2".into(),
            from: "demo".into(),
        },
        cooldown::Requirement {
            crate_name: "fresh-only".into(),
            req: "^2.0".into(),
            from: "demo".into(),
        },
    ];
    let now = "2026-11-12T00:00:00Z".parse().unwrap();
    let (findings, summary) = cooldown::preview(&lock, &reqs, &index, "7 days", now).unwrap();
    assert_eq!(
        (
            summary.registry_packages,
            summary.other_sources,
            summary.young,
            summary.unsatisfiable
        ),
        (4, 1, 3, 1)
    );
    let got: Vec<_> = findings
        .iter()
        .map(|f| (f.line, f.rule, f.severity))
        .collect();
    assert_eq!(
        got,
        vec![
            (6, cooldown::YOUNG, Severity::Note),
            (20, cooldown::UNSATISFIABLE, Severity::Error),
            (37, cooldown::YOUNG, Severity::Note),
        ]
    );
    // young-dep 1.2.5 needs chain-child ^0.6.5, but its fallback 1.2.3 accepts ^0.6.0
    // (its optional renamed `chain-old = { package = "chain-child", version = "^0.3" }` is not active).
    assert!(
        findings[0].message.contains("would pick 0.6.4"),
        "{}",
        findings[0].message
    );
    assert!(
        findings[1].message.contains("`^2.0` from demo"),
        "{}",
        findings[1].message
    );
    // 1.2.4 is yanked, so the fallback for young-dep is 1.2.3.
    assert!(
        findings[2].message.contains("would pick 1.2.3"),
        "{}",
        findings[2].message
    );
}

#[test]
fn cooldown_reports_missing_index_data() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cooldown");
    let lock = std::fs::read_to_string(dir.join("Cargo.lock")).unwrap();
    let now = "2026-11-12T00:00:00Z".parse().unwrap();
    let err = cooldown::preview(&lock, &[], &HashMap::new(), "7 days", now).unwrap_err();
    assert!(err.contains("no index data"), "{err}");
}

#[test]
fn inline_ignore_on_same_or_previous_line() {
    assert_eq!(
        scan("ignored"),
        vec![hit(".github/workflows/ci.yml", 6, TRUNC, Severity::Error)]
    );
}

#[test]
fn fix_quotes_versions_in_place() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/actions-rs-matrix");
    let dir = std::env::temp_dir().join(format!("rust1100-check-fix-{}", std::process::id()));
    let wf = dir.join(".github/workflows");
    std::fs::create_dir_all(&wf).unwrap();
    std::fs::copy(src.join(".github/workflows/CI.yml"), wf.join("CI.yml")).unwrap();

    let (findings, _) = scan_static(&dir).unwrap();
    assert_eq!(rust1100_check::fix_yaml(&dir, &findings).unwrap(), 5);
    let text = std::fs::read_to_string(wf.join("CI.yml")).unwrap();
    assert!(text.contains("toolchain: \"1.88\""), "{text}");
    assert!(
        text.contains("rust: [\"1.93\", \"1.94\", \"1.95\", \"1.96\"]"),
        "{text}"
    );
    assert!(scan_static(&dir).unwrap().0.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

const I686: &str = "i686-windows-host-toolchain";

#[test]
fn i686_windows_host_toolchains() {
    let wf = ".github/workflows/ci.yml";
    assert_eq!(
        scan("i686-host"),
        vec![
            hit(wf, 15, I686, Severity::Error),
            hit(wf, 19, I686, Severity::Error),
            hit(wf, 32, I686, Severity::Error),
            hit(wf, 37, I686, Severity::Error),
            hit(wf, 38, I686, Severity::Error),
            hit("sub/rust-toolchain.toml", 2, I686, Severity::Error),
        ]
    );
}

#[test]
fn i686_windows_cross_compiling_is_not_reported() {
    assert_eq!(scan("i686-cross"), vec![]);
}
