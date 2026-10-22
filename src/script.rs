//! R3/R4: scripts that compare or match Rust versions in ways that break at 1.100.

use std::sync::LazyLock;

use regex::Regex;

use crate::{Finding, Severity};

pub const STRING_COMPARE: &str = "version-compare-as-string";
pub const TWO_DIGIT: &str = "two-digit-minor-pattern";

static CONTEXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)rustc|rust[_-]?version|msrv|toolchain").unwrap());
/// `VAR=$(rustc -V)`, `VAR := $(shell rustc --version)`, `var = run(["rustc", "-V"])`, at the start of a line.
static RUSTC_ASSIGN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*(?:export\s+|local\s+|let\s+|const\s+|var\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*:?=.*\brustc["']?\s*,?\s*["']?(?:-vV|-V|--version)"#)
        .unwrap()
});

/// (pattern, what is wrong, how to fix)
static COMPARES: LazyLock<Vec<(Regex, &str, &str)>> = LazyLock::new(|| {
    const NUMERIC: &str =
        "compare the minor version as an integer (`cut -d. -f2`, then `-ge`), or use `sort -V`";
    [
        (r"\[\[[^\]]*\s[<>]\s", "`[[ a < b ]]` compares strings, and \"1.100\" sorts before \"1.99\"", NUMERIC),
        (r"\\[<>]", "`[ a \\< b ]` compares strings, and \"1.100\" sorts before \"1.99\"", NUMERIC),
        (
            r#"[<>]=?\s*["']\d+\.\d+|["']\d+\.\d+(?:\.\d+)?["']\s*[<>]"#,
            "comparison against a quoted version literal is a string comparison, and \"1.100\" sorts before \"1.99\"",
            "parse the version (`semver`, `packaging.version`, `rustc_version`) or compare the minor as an integer",
        ),
        (
            r"\|\s*bc\b|\bawk\b[^|]*'[^']*[<>][^']*'|\bfloat\(|parseFloat\(",
            "comparing the version as a decimal number reads 1.100 as 1.1",
            NUMERIC,
        ),
        (
            // `-V` sorts versions correctly; `-u` alone is usually just de-duplication.
            r"(?:^|[|;&(]\s*)sort(?:\s+-[a-tv-zA-UW-Z]+)*\s*(?:$|[|;)])",
            "`sort` without `-V` puts 1.100 before 1.99",
            "use `sort -V` (or `sort -t. -k1,1n -k2,2n -k3,3n`)",
        ),
        (
            r#"\bcut\s+(?:-c\s*|--characters=)(?:\d*-\d+|\d+)(?:[\s|;)'"]|$)|\$\{\w+:\d+:\d+\}|\[:\d+\]|\[\.\.\d+\]|\.substr(?:ing)?\("#,
            "fixed-width slicing cuts 1.100 down to 1.10",
            "split on `.` instead of slicing by character position",
        ),
    ]
    .into_iter()
    .map(|(re, what, fix)| (Regex::new(re).unwrap(), what, fix))
    .collect()
});

/// `1\.[0-9]{2}`, `1\.\d{2}`, `1\.[0-9][0-9]`, `1\.\d\d`, BRE `1\.[0-9]\{2\}`.
static TWO_DIGIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"1\\\.(?:\[0-9\]|\\d)(?:\{2\}|\\\{2\\\}|\{2,2\}|(\[0-9\]|\\d))").unwrap()
});

pub fn check(file: &str, text: &str, line_offset: usize, out: &mut Vec<Finding>) {
    let vars: Vec<Regex> = text
        .lines()
        .filter_map(|l| RUSTC_ASSIGN.captures(l))
        .map(|c| Regex::new(&format!(r"\b{}\b", regex::escape(&c[1]))).unwrap())
        .collect();
    for (idx, line) in text.lines().enumerate() {
        let lineno = idx + 1 + line_offset;
        if let Some(m) = TWO_DIGIT_RE.captures(line) {
            let whole = m.get(0).unwrap();
            let rest = &line[whole.end()..];
            // A trailing quantifier (`[0-9][0-9]+`, `\d\d*`) already accepts more digits.
            let open_ended = (m.get(1).is_some() && rest.starts_with(['+', '*', '{', '?']))
                || rest.starts_with("\\{");
            if !open_ended {
                out.push(Finding {
                    rule: TWO_DIGIT,
                    severity: Severity::Warning,
                    file: file.to_owned(),
                    line: lineno,
                    column: whole.start() + 1,
                    message: format!("`{}` only matches a two-digit minor version, so 1.100 fails or is cut to 1.10", whole.as_str()),
                    help: "use `1\\.[0-9]+`".to_owned(),
                });
            }
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if !CONTEXT.is_match(line) && !vars.iter().any(|v| v.is_match(line)) {
            continue;
        }
        if let Some((m, what, fix)) = COMPARES
            .iter()
            .find_map(|(re, w, f)| re.find(line).map(|m| (m, w, f)))
        {
            out.push(Finding {
                rule: STRING_COMPARE,
                severity: Severity::Warning,
                file: file.to_owned(),
                line: lineno,
                column: m.start() + 1,
                message: format!("{what}: `{}`", trimmed.trim_end()),
                help: (*fix).to_owned(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(text: &str) -> Vec<(&'static str, usize)> {
        let mut out = Vec::new();
        check("x.sh", text, 0, &mut out);
        out.iter().map(|f| (f.rule, f.line)).collect()
    }

    #[test]
    fn needs_rust_context() {
        assert!(hits("if [[ \"$a\" < \"$b\" ]]; then :; fi\nls | sort\n").is_empty());
    }

    #[test]
    fn tracks_variables_assigned_from_rustc() {
        let text =
            "v=$(rustc --version | cut -d' ' -f2)\nif [[ \"$v\" < \"1.70\" ]]; then exit 1; fi\n";
        assert_eq!(hits(text), vec![(STRING_COMPARE, 2)]);
    }

    #[test]
    fn correct_forms_pass() {
        let text = "rustc -V | sort -V\nrustc -V | sort -t. -k2,2n\nrustc -V | cut -d' ' -f2\n";
        assert!(hits(text).is_empty(), "{:?}", hits(text));
    }

    #[test]
    fn real_world_false_positives_stay_quiet() {
        // From Analog-Labs/chain-connectors, npryce/rusty-pi, olu-py/1H-Agent-TUI, d0bromir/shmap-rs.
        let text = r#"rustc_version="$(awk '{print $2}' <<< "${rustc_version}")"
command -v awk >/dev/null 2>&1 || exit 2
rust_version:=$(shell rustc --version | cut -c 7-)
declared="$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml | sort -u)"
    commit=commit, rustc=rustc_version(),
wt = WORKROOT / commit[:12]
"#;
        assert!(hits(text).is_empty(), "{:?}", hits(text));
    }

    #[test]
    fn sort_and_float() {
        let text = "printf '%s\\n' \"$MSRV\" \"$(rustc -V)\" | sort | head -1\necho \"$RUST_VERSION >= 1.70\" | bc\n";
        assert_eq!(hits(text), vec![(STRING_COMPARE, 1), (STRING_COMPARE, 2)]);
    }

    #[test]
    fn two_digit_patterns() {
        let text =
            "grep -E '1\\.[0-9]{2}'\nre = r'1\\.\\d\\d'\nok='1\\.[0-9][0-9]*'\nok2='1\\.[0-9]+'\n";
        assert_eq!(hits(text), vec![(TWO_DIGIT, 1), (TWO_DIGIT, 2)]);
    }
}
