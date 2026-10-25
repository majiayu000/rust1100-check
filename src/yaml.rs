//! R1/R2: unquoted version numbers in CI YAML, plus extraction of `run:` blocks for script checks.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use yaml_rust2::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust2::scanner::{Marker, TScalarStyle};

use crate::{Finding, Severity, i686, script};

pub const TRUNCATED: &str = "yaml-float-version-truncated";
pub const FRAGILE: &str = "yaml-float-rust-version";
pub const UNPARSABLE: &str = "unparsable-yaml";

/// Keys whose values become shell scripts.
const SCRIPT_KEYS: &[&str] = &["run", "script", "before_script", "after_script", "command"];

static FLOAT_TRAILING_ZERO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d+\.\d*0$").unwrap());
static RUST_TWO_PART: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^1\.\d+$").unwrap());
static QUOTED_VERSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"['"]\d+\.\d+"#).unwrap());

#[derive(Debug)]
enum Node {
    Scalar {
        value: String,
        style: TScalarStyle,
        line: usize,
        col: usize,
    },
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
    Alias,
}

impl Node {
    fn as_str(&self) -> Option<&str> {
        match self {
            Node::Scalar { value, .. } => Some(value),
            _ => None,
        }
    }

    fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(entries) => entries
                .iter()
                .find(|(k, _)| k.as_str() == Some(key))
                .map(|(_, v)| v),
            _ => None,
        }
    }
}

enum Frame {
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>, Option<Node>),
}

#[derive(Default)]
struct Builder {
    stack: Vec<Frame>,
    docs: Vec<Node>,
}

impl Builder {
    fn push(&mut self, node: Node) {
        match self.stack.last_mut() {
            Some(Frame::Seq(items)) => items.push(node),
            Some(Frame::Map(entries, pending)) => match pending.take() {
                Some(key) => entries.push((key, node)),
                None => *pending = Some(node),
            },
            None => self.docs.push(node),
        }
    }
}

impl MarkedEventReceiver for Builder {
    fn on_event(&mut self, ev: Event, mark: Marker) {
        match ev {
            Event::Scalar(value, style, _, _) => self.push(Node::Scalar {
                value,
                style,
                line: mark.line(),
                col: mark.col() + 1,
            }),
            Event::Alias(_) => self.push(Node::Alias),
            Event::SequenceStart(..) => self.stack.push(Frame::Seq(Vec::new())),
            Event::MappingStart(..) => self.stack.push(Frame::Map(Vec::new(), None)),
            Event::SequenceEnd => {
                if let Some(Frame::Seq(items)) = self.stack.pop() {
                    self.push(Node::Seq(items));
                }
            }
            Event::MappingEnd => {
                if let Some(Frame::Map(entries, _)) = self.stack.pop() {
                    self.push(Node::Map(entries));
                }
            }
            _ => {}
        }
    }
}

fn parse(text: &str) -> Result<Vec<Node>, String> {
    let mut builder = Builder::default();
    Parser::new_from_str(text)
        .load(&mut builder, true)
        .map_err(|e| e.to_string())?;
    Ok(builder.docs)
}

fn is_rust_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    ["rust", "toolchain", "msrv"].iter().any(|w| k.contains(w))
}

fn is_version_key(key: &str) -> bool {
    is_rust_key(key) || key.to_ascii_lowercase().contains("version")
}

#[derive(Clone, Default)]
struct Ctx<'a> {
    /// Inside `with:` of a `uses: dtolnay/rust-toolchain@…` step.
    dtolnay_with: bool,
    /// Inside a job's `strategy:` block.
    in_strategy: bool,
    /// Matrix keys of the current job whose every use is a dtolnay `toolchain` input.
    safe_matrix_keys: Option<&'a HashSet<String>>,
    /// Inside the `env:` block of a workflow or job.
    in_env: bool,
    /// Env keys of that workflow or job whose every use is a dtolnay `toolchain` input.
    safe_env_keys: Option<&'a HashSet<String>>,
}

fn uses_dtolnay(step: &Node) -> bool {
    step.get("uses")
        .and_then(Node::as_str)
        .is_some_and(|u| u.starts_with("dtolnay/rust-toolchain"))
}

/// A job-like mapping (GitHub Actions job): has `steps` or `strategy`.
fn is_job(node: &Node) -> bool {
    node.get("steps").is_some() || node.get("strategy").is_some()
}

/// Collect every string in `node` together with whether its position is harmless for a
/// float-truncated matrix value (dtolnay toolchain input, a name, or a condition that does
/// not compare against a version literal).
fn collect_refs<'n>(
    node: &'n Node,
    key: Option<&str>,
    dtolnay_with: bool,
    out: &mut Vec<(&'n str, bool)>,
) {
    match node {
        Node::Scalar { value, .. } => {
            let safe = match key {
                Some("toolchain") => dtolnay_with,
                Some("name") => true,
                Some("if" | "continue-on-error") => !QUOTED_VERSION.is_match(value),
                _ => false,
            };
            out.push((value, safe));
        }
        Node::Seq(items) => items
            .iter()
            .for_each(|i| collect_refs(i, key, dtolnay_with, out)),
        Node::Map(entries) => {
            for (k, v) in entries {
                let k = k.as_str();
                let in_with = k == Some("with") && uses_dtolnay(node);
                collect_refs(v, k, dtolnay_with || in_with, out);
            }
        }
        Node::Alias => {}
    }
}

/// Keys from `keys` whose every reference in `scope` (outside its `skip` block) sits in a
/// harmless position. A key with no references at all is not considered safe.
fn safe_keys(
    scope: &Node,
    skip: &str,
    keys: HashSet<String>,
    ref_pattern: impl Fn(&str) -> String,
) -> HashSet<String> {
    let mut refs = Vec::new();
    if let Node::Map(entries) = scope {
        for (k, v) in entries {
            if k.as_str() != Some(skip) {
                collect_refs(v, k.as_str(), false, &mut refs);
            }
        }
    }
    keys.into_iter()
        .filter(|key| {
            let pat = Regex::new(&ref_pattern(&regex::escape(key))).unwrap();
            let mut hits = refs.iter().filter(|(s, _)| pat.is_match(s)).peekable();
            hits.peek().is_some() && hits.all(|(_, safe)| *safe)
        })
        .collect()
}

fn map_keys(node: &Node, keys: &mut HashSet<String>) {
    if let Node::Map(entries) = node {
        keys.extend(
            entries
                .iter()
                .filter_map(|(k, _)| k.as_str().map(str::to_owned)),
        );
    }
}

fn safe_matrix_keys(job: &Node) -> HashSet<String> {
    let Some(matrix) = job.get("strategy").and_then(|s| s.get("matrix")) else {
        return HashSet::new();
    };
    let mut keys = HashSet::new();
    map_keys(matrix, &mut keys);
    if let Some(Node::Seq(items)) = matrix.get("include") {
        items.iter().for_each(|i| map_keys(i, &mut keys));
    }
    safe_keys(job, "strategy", keys, |k| {
        format!(r"matrix\.{k}(?:[^A-Za-z0-9_-]|$)")
    })
}

/// Env keys of a workflow or job; `$KEY` in a script counts as an unsafe reference.
fn safe_env_keys(scope: &Node) -> HashSet<String> {
    let mut keys = HashSet::new();
    if let Some(env) = scope.get("env") {
        map_keys(env, &mut keys);
    }
    safe_keys(scope, "env", keys, |k| {
        format!(r"(?:env\.{k}|\$\{{?{k})(?:[^A-Za-z0-9_]|$)")
    })
}

static MATRIX_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{\{\s*matrix\.([A-Za-z0-9_.-]+)\s*\}\}").unwrap());

/// Every scalar a job's matrix can give `matrix.<path>`, from its axes and `include` entries.
fn matrix_values(job: &Node) -> HashMap<String, Vec<String>> {
    fn add(path: &str, node: &Node, vals: &mut HashMap<String, Vec<String>>) {
        match node {
            Node::Scalar { value, .. } => {
                let list = vals.entry(path.to_owned()).or_default();
                if !list.contains(value) {
                    list.push(value.clone());
                }
            }
            Node::Seq(items) => items.iter().for_each(|i| add(path, i, vals)),
            Node::Map(entries) => {
                for (k, v) in entries {
                    let Some(k) = k.as_str() else { continue };
                    let sub = if path.is_empty() {
                        k.to_owned()
                    } else {
                        format!("{path}.{k}")
                    };
                    add(&sub, v, vals);
                }
            }
            Node::Alias => {}
        }
    }
    let mut vals = HashMap::new();
    if let Some(Node::Map(entries)) = job.get("strategy").and_then(|s| s.get("matrix")) {
        for (k, v) in entries {
            match k.as_str() {
                Some("include") => add("", v, &mut vals),
                Some("exclude") | None => {}
                Some(k) => add(k, v, &mut vals),
            }
        }
    }
    vals
}

/// All strings `template` can become with matrix values filled in (capped). Empty when a
/// placeholder has no known values.
fn expand(template: &str, vals: &HashMap<String, Vec<String>>) -> Vec<String> {
    const CAP: usize = 256;
    let mut outs = vec![template.to_owned()];
    let mut done = HashSet::new();
    for c in MATRIX_REF.captures_iter(template) {
        let placeholder = c.get(0).unwrap().as_str();
        if !done.insert(placeholder) {
            continue;
        }
        let Some(values) = vals.get(&c[1]) else {
            return Vec::new();
        };
        outs = outs
            .iter()
            .flat_map(|o| values.iter().map(move |v| o.replace(placeholder, v)))
            .take(CAP)
            .collect();
    }
    outs
}

fn collect_lines<'n>(node: &'n Node, out: &mut Vec<(&'n str, usize)>) {
    match node {
        Node::Scalar { value, line, .. } => out.push((value, *line)),
        Node::Seq(items) => items.iter().for_each(|i| collect_lines(i, out)),
        // Display names only echo the toolchain; the step that installs it is what matters.
        Node::Map(entries) => entries
            .iter()
            .filter(|(k, _)| k.as_str() != Some("name"))
            .for_each(|(_, v)| collect_lines(v, out)),
        Node::Alias => {}
    }
}

struct Walker<'f> {
    file: &'f str,
    text: &'f str,
    out: &'f mut Vec<Finding>,
    scripts: Vec<(String, usize)>,
}

impl Walker<'_> {
    fn walk(&mut self, node: &Node, key: Option<&str>, ctx: &Ctx) {
        match node {
            Node::Scalar {
                value,
                style,
                line,
                col,
            } => {
                let Some(key) = key else { return };
                if SCRIPT_KEYS.contains(&key) {
                    // For `|`/`>` blocks the marker is already on the first content line.
                    self.scripts.push((value.clone(), *line));
                }
                if *style == TScalarStyle::Plain {
                    self.check_scalar(value, key, *line, *col, ctx);
                }
            }
            Node::Seq(items) => items.iter().for_each(|i| self.walk(i, key, ctx)),
            Node::Map(entries) => {
                let job = is_job(node);
                if job {
                    self.check_matrix_toolchains(node);
                }
                let job_keys = job.then(|| safe_matrix_keys(node));
                let env_keys = (job || node.get("jobs").is_some()).then(|| safe_env_keys(node));
                let mut base = ctx.clone();
                if let Some(keys) = &job_keys {
                    base.safe_matrix_keys = Some(keys);
                    base.in_strategy = false;
                }
                if env_keys.is_some() {
                    base.in_env = false;
                }
                for (k, v) in entries {
                    let Some(kname) = k.as_str() else { continue };
                    let mut ctx = base.clone();
                    if kname == "with" && uses_dtolnay(node) {
                        ctx.dtolnay_with = true;
                    }
                    if kname == "strategy" && job_keys.is_some() {
                        ctx.in_strategy = true;
                    }
                    if kname == "env" && env_keys.is_some() {
                        ctx.in_env = true;
                        ctx.safe_env_keys = env_keys.as_ref();
                    }
                    self.walk(v, Some(kname), &ctx);
                }
            }
            Node::Alias => {}
        }
    }

    /// R7 on toolchain names assembled from matrix values, e.g.
    /// `toolchain: ${{ matrix.version }}-${{ matrix.toolchain }}` with `toolchain: i686-pc-windows-msvc`.
    fn check_matrix_toolchains(&mut self, job: &Node) {
        let vals = matrix_values(job);
        if vals.is_empty() {
            return;
        }
        let mut strings = Vec::new();
        if let Node::Map(entries) = job {
            for (k, v) in entries {
                if k.as_str() != Some("strategy") {
                    collect_lines(v, &mut strings);
                }
            }
        }
        for (value, first_line) in strings {
            for (idx, line) in value.lines().enumerate() {
                if !MATRIX_REF.is_match(line) {
                    continue;
                }
                for expanded in expand(line, &vals) {
                    // A name spelled out literally (e.g. as the matrix value itself) is
                    // already reported where it is written.
                    let found = i686::mentions(&expanded);
                    if !found.is_empty() && found.iter().any(|m| !self.text.contains(m)) {
                        i686::check_line(
                            self.file,
                            &expanded,
                            first_line + idx,
                            Some(line),
                            self.out,
                        );
                    }
                }
            }
        }
    }

    fn check_scalar(&mut self, value: &str, key: &str, line: usize, col: usize, ctx: &Ctx) {
        let rust = is_rust_key(key);
        let mitigated = rust
            && ((ctx.dtolnay_with && key == "toolchain")
                || (ctx.in_strategy && ctx.safe_matrix_keys.is_some_and(|k| k.contains(key)))
                || (ctx.in_env && ctx.safe_env_keys.is_some_and(|k| k.contains(key))));
        let loaded = value.trim_end_matches('0');
        let loaded = loaded.strip_suffix('.').unwrap_or(loaded);
        let (rule, severity, message) = if is_version_key(key)
            && FLOAT_TRAILING_ZERO.is_match(value)
        {
            let severity = match (rust, mitigated) {
                (true, true) => Severity::Note,
                (true, false) => Severity::Error,
                (false, _) => Severity::Warning,
            };
            let mut message =
                format!("`{key}: {value}` is an unquoted YAML number and is loaded as {loaded}");
            if mitigated {
                message.push_str("; dtolnay/rust-toolchain restores the zero for its `toolchain` input, but anything else reading this value sees the truncated number");
            } else if rust {
                message.push_str(&format!("; rustup would install Rust {loaded}.0"));
            }
            (TRUNCATED, severity, message)
        } else if rust && !mitigated && RUST_TWO_PART.is_match(value) {
            let message = format!(
                "`{key}: {value}` is an unquoted YAML number; it works today, but bumping it to 1.100 loads as 1.1 and installs Rust 1.1.0"
            );
            (FRAGILE, Severity::Warning, message)
        } else {
            return;
        };
        self.out.push(Finding {
            rule,
            severity,
            file: self.file.to_owned(),
            line,
            column: col,
            message,
            help: format!(
                "quote it: `{key}: \"{value}\"`, or use a full version such as `{value}.0`"
            ),
        });
    }
}

/// Check one CI YAML file: R1/R2 on values, R3/R4 on embedded scripts.
pub fn check(file: &str, text: &str, out: &mut Vec<Finding>) {
    let docs = match parse(text) {
        Ok(docs) => docs,
        Err(e) => {
            out.push(Finding {
                rule: UNPARSABLE,
                severity: Severity::Warning,
                file: file.to_owned(),
                line: 1,
                column: 1,
                message: format!("could not parse YAML, file was not checked: {e}"),
                help: "fix the YAML syntax (templated files are not supported)".to_owned(),
            });
            return;
        }
    };
    let mut walker = Walker {
        file,
        text,
        out,
        scripts: Vec::new(),
    };
    for doc in &docs {
        walker.walk(doc, None, &Ctx::default());
    }
    let scripts = std::mem::take(&mut walker.scripts);
    for (body, first_line) in scripts {
        script::check(file, &body, first_line - 1, out);
    }
}

/// Quote the unquoted version numbers reported by R1/R2 in one file. `positions` are the
/// (line, column) pairs of those findings, 1-based, column counted in characters.
/// Returns the new text and how many values were quoted.
pub fn quote_versions(text: &str, positions: &[(usize, usize)]) -> (String, usize) {
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let mut sorted = positions.to_vec();
    // Right to left, so earlier columns on the same line stay valid.
    sorted.sort_by(|a, b| b.cmp(a));
    sorted.dedup();
    let mut fixed = 0;
    for (line, col) in sorted {
        let Some(l) = lines.get_mut(line - 1) else {
            continue;
        };
        let Some((start, _)) = l.char_indices().nth(col - 1) else {
            continue;
        };
        let len = l[start..]
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(l.len() - start);
        let value = &l[start..start + len];
        if FLOAT_TRAILING_ZERO.is_match(value) || RUST_TWO_PART.is_match(value) {
            *l = format!("{}\"{value}\"{}", &l[..start], &l[start + len..]);
            fixed += 1;
        }
    }
    (lines.join("\n"), fixed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str) -> Vec<(&'static str, Severity, usize)> {
        let mut out = Vec::new();
        check("ci.yml", text, &mut out);
        out.iter().map(|f| (f.rule, f.severity, f.line)).collect()
    }

    #[test]
    fn plain_floats_only() {
        let text = "env:\n  RUST_VERSION: 1.90\n  MSRV: \"1.90\"\n  TOOLCHAIN: 1.90.0\n  PY_VERSION: 3.10\n  OTHER: 1.90\n";
        assert_eq!(
            rules(text),
            vec![
                (TRUNCATED, Severity::Error, 2),
                (TRUNCATED, Severity::Warning, 5)
            ]
        );
    }

    #[test]
    fn dtolnay_matrix_is_mitigated() {
        let text = r#"
jobs:
  test:
    strategy:
      matrix:
        rust: [1.90, 1.95, stable]
    name: test ${{ matrix.rust }}
    steps:
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
      - run: cargo test
"#;
        assert_eq!(rules(text), vec![(TRUNCATED, Severity::Note, 6)]);
    }

    #[test]
    fn matrix_used_by_rustup_is_not_mitigated() {
        let text = r#"
jobs:
  test:
    strategy:
      matrix:
        rust: [1.95]
    steps:
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: ${{ matrix.rust }}
      - run: cargo +${{ matrix.rust }} test
"#;
        assert_eq!(rules(text), vec![(FRAGILE, Severity::Warning, 6)]);
    }

    #[test]
    fn env_used_only_by_dtolnay_is_mitigated() {
        // Minimized from tokio-rs/tokio .github/workflows/stress-test.yml
        let text = r#"
env:
  rust_stable: 1.85
  MSRV: 1.90
jobs:
  stess-test:
    runs-on: ubuntu-latest
    steps:
      - name: Install Rust ${{ env.rust_stable }}
        uses: dtolnay/rust-toolchain@master
        with:
            toolchain: ${{ env.rust_stable }}
      - run: cargo +$MSRV check
"#;
        assert_eq!(rules(text), vec![(TRUNCATED, Severity::Error, 4)]);
    }

    #[test]
    fn quote_versions_rewrites_only_reported_values() {
        let text = "rust: [1.90, 1.95, stable] # 1.90\ntoolchain: 1.100\nother: 1.5\n";
        let mut out = Vec::new();
        check("ci.yml", text, &mut out);
        let positions: Vec<_> = out.iter().map(|f| (f.line, f.column)).collect();
        let (fixed, n) = quote_versions(text, &positions);
        assert_eq!(n, 3);
        assert_eq!(
            fixed,
            "rust: [\"1.90\", \"1.95\", stable] # 1.90\ntoolchain: \"1.100\"\nother: 1.5\n"
        );
        let mut again = Vec::new();
        check("ci.yml", &fixed, &mut again);
        assert!(again.is_empty());
    }

    #[test]
    fn run_block_lines_map_to_file() {
        let text = "jobs:\n  a:\n    steps:\n      - run: |\n          echo hi\n          if [[ \"$(rustc -V)\" < \"rustc 1.70\" ]]; then exit 1; fi\n";
        let mut out = Vec::new();
        check("ci.yml", text, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].rule, out[0].line), (script::STRING_COMPARE, 6));
    }
}
