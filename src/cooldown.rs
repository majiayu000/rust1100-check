//! R5: preview what Cargo's `registry.global-min-publish-age` (stable in 1.100) would hold back.
//!
//! Cargo ignores versions whose index `pubtime` is newer than the configured age, unless the
//! version is already in Cargo.lock. So enabling it does not break an existing lockfile; it
//! changes what `cargo update`, new dependencies, and lockfile-less CI resolve to.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use jiff::{SignedDuration, Timestamp};
use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::{Finding, Severity};

pub const UNSATISFIABLE: &str = "cooldown-unsatisfiable";
pub const YOUNG: &str = "cooldown-young-locked";

const CRATES_IO_SOURCES: &[&str] = &[
    "registry+https://github.com/rust-lang/crates.io-index",
    "sparse+https://index.crates.io/",
];
pub const SPARSE_INDEX: &str = "https://index.crates.io/";

/// Parse Cargo's min-publish-age syntax: `"0"` or `"<N> <unit>"`.
pub fn parse_age(s: &str) -> Result<SignedDuration, String> {
    let s = s.trim();
    if s == "0" {
        return Ok(SignedDuration::ZERO);
    }
    let err = || {
        format!(
            "invalid min-publish-age `{s}`: expected \"0\" or \"<N> seconds|minutes|hours|days|weeks|months\""
        )
    };
    let (n, unit) = s.split_once(char::is_whitespace).ok_or_else(err)?;
    let n: i64 = n.parse().map_err(|_| err())?;
    let secs = match unit.trim().trim_end_matches('s') {
        "second" => 1,
        "minute" => 60,
        "hour" => 3600,
        "day" => 86_400,
        "week" => 7 * 86_400,
        // Cargo's exact month length is not documented; 30 days is used here.
        "month" => 30 * 86_400,
        _ => return Err(err()),
    };
    Ok(SignedDuration::from_secs(n * secs))
}

#[derive(Deserialize)]
struct Lock {
    #[serde(default)]
    package: Vec<LockPackage>,
}

#[derive(Deserialize)]
struct LockPackage {
    name: String,
    version: String,
    source: Option<String>,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Deserialize)]
pub struct IndexEntry {
    vers: String,
    #[serde(default)]
    deps: Vec<IndexDep>,
    #[serde(default)]
    yanked: bool,
    pubtime: Option<String>,
}

#[derive(Deserialize)]
struct IndexDep {
    name: String,
    package: Option<String>,
    req: String,
    kind: Option<String>,
    #[serde(default)]
    optional: bool,
}

impl IndexDep {
    fn crate_name(&self) -> &str {
        self.package.as_deref().unwrap_or(&self.name)
    }
}

/// A version requirement on a crate and who imposes it.
pub struct Requirement {
    pub crate_name: String,
    pub req: String,
    pub from: String,
}

#[derive(Default, Debug)]
pub struct Summary {
    pub age: String,
    pub registry_packages: usize,
    pub other_sources: usize,
    pub young: usize,
    pub unsatisfiable: usize,
}

pub fn sparse_path(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    match n.len() {
        1 => format!("1/{n}"),
        2 => format!("2/{n}"),
        3 => format!("3/{}/{n}", &n[..1]),
        _ => format!("{}/{}/{n}", &n[..2], &n[2..4]),
    }
}

/// Fetch a crate's index file from the crates.io sparse index.
pub fn fetch_sparse(agent: &ureq::Agent, name: &str) -> Result<String, String> {
    let url = format!("{SPARSE_INDEX}{}", sparse_path(name));
    agent
        .get(&url)
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("reading {url}: {e}"))
}

fn parse_index(name: &str, body: &str) -> Result<Vec<IndexEntry>, String> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|e| format!("index entry for {name}: {e}")))
        .collect()
}

/// Requirements declared by workspace members, from `cargo metadata --no-deps`.
pub fn workspace_requirements(root: &Path) -> Result<Vec<Requirement>, String> {
    #[derive(Deserialize)]
    struct Meta {
        packages: Vec<MetaPkg>,
    }
    #[derive(Deserialize)]
    struct MetaPkg {
        name: String,
        dependencies: Vec<MetaDep>,
    }
    #[derive(Deserialize)]
    struct MetaDep {
        name: String,
        source: Option<String>,
        req: String,
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .output()
        .map_err(|e| format!("running cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let meta: Meta = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("cargo metadata output: {e}"))?;
    Ok(meta
        .packages
        .into_iter()
        .flat_map(|p| {
            let from = p.name;
            p.dependencies
                .into_iter()
                .filter(|d| {
                    d.source
                        .as_deref()
                        .is_some_and(|s| CRATES_IO_SOURCES.contains(&s))
                })
                .map(move |d| Requirement {
                    crate_name: d.name,
                    req: d.req,
                    from: from.clone(),
                })
        })
        .collect())
}

/// Fetch index files for `names` with a few worker threads.
pub fn fetch_all(
    names: &[String],
    fetch: &(dyn Fn(&str) -> Result<String, String> + Sync),
) -> Result<HashMap<String, String>, String> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new(HashMap::new());
    let first_err = Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..names.len().clamp(1, 16) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(name) = names.get(i) else { break };
                    match fetch(name) {
                        Ok(body) => {
                            results.lock().unwrap().insert(name.clone(), body);
                        }
                        Err(e) => {
                            first_err.lock().unwrap().get_or_insert(e);
                        }
                    }
                }
            });
        }
    });
    match first_err.into_inner().unwrap() {
        Some(e) => Err(e),
        None => Ok(results.into_inner().unwrap()),
    }
}

fn find_lock_line(lock_text: &str, name: &str, version: &str) -> usize {
    let needle = format!("name = \"{name}\"\nversion = \"{version}\"");
    lock_text
        .find(&needle)
        .map(|pos| lock_text[..pos].lines().count() + 1)
        .unwrap_or(1)
}

fn days(d: SignedDuration) -> String {
    let h = d.as_hours();
    if h < 48 {
        format!("{h} hours")
    } else {
        format!("{} days", h / 24)
    }
}

struct Candidate<'a> {
    version: Version,
    pubtime: Option<Timestamp>,
    entry: &'a IndexEntry,
}

type Reqs = HashMap<usize, Vec<(VersionReq, String)>>;

/// Requirements on each locked registry package (by index into `registry`), taken from the
/// workspace manifests and from each package's effective (locked or fallback) index entry.
fn requirements(
    registry: &[&LockPackage],
    effective: &[&Candidate],
    locked: &[&Candidate],
    workspace_reqs: &[Requirement],
) -> Result<Reqs, String> {
    let mut reqs: Reqs = HashMap::new();
    let mut add = |name: &str, req_text: &str, from: String| -> Result<(), String> {
        let req = VersionReq::parse(req_text)
            .map_err(|e| format!("requirement `{req_text}` on {name}: {e}"))?;
        let same_name: Vec<usize> = (0..registry.len())
            .filter(|&i| registry[i].name == name)
            .collect();
        for &i in &same_name {
            // With several locked versions of a crate, attach the requirement to the one it selects.
            if same_name.len() == 1 || req.matches(&locked[i].version) {
                reqs.entry(i).or_default().push((req.clone(), from.clone()));
            }
        }
        Ok(())
    };
    for r in workspace_reqs {
        add(&r.crate_name, &r.req, r.from.clone())?;
    }
    for (j, p) in registry.iter().enumerate() {
        let from = if std::ptr::eq(effective[j], locked[j]) {
            format!("{} {}", p.name, p.version)
        } else {
            format!("{} {} (its fallback)", p.name, effective[j].version)
        };
        for dep in &p.dependencies {
            let dep_name = dep.split(' ').next().unwrap_or(dep);
            let cands: Vec<&IndexDep> = effective[j]
                .entry
                .deps
                .iter()
                .filter(|d| d.crate_name() == dep_name && d.kind.as_deref() != Some("dev"))
                .collect();
            // A crate can appear several times (renamed optional deps such as combine's
            // `tokio-02`/`tokio-03`). Keep the entries that select a locked version; if none
            // do (a fallback parent), keep the non-optional ones.
            let selects_locked = |d: &&IndexDep| {
                VersionReq::parse(&d.req).is_ok_and(|r| {
                    (0..registry.len())
                        .any(|i| registry[i].name == dep_name && r.matches(&locked[i].version))
                })
            };
            let mut active: Vec<&IndexDep> = cands.iter().copied().filter(selects_locked).collect();
            if active.is_empty() {
                active = cands.into_iter().filter(|d| !d.optional).collect();
            }
            for d in active {
                add(dep_name, &d.req, from.clone())?;
            }
        }
    }
    Ok(reqs)
}

/// Run the preview against a Cargo.lock. `index` maps crate name to its sparse index file body.
pub fn preview(
    lock_text: &str,
    workspace_reqs: &[Requirement],
    index: &HashMap<String, String>,
    age_text: &str,
    now: Timestamp,
) -> Result<(Vec<Finding>, Summary), String> {
    let age = parse_age(age_text)?;
    let cutoff = now
        .checked_sub(age)
        .map_err(|e| format!("min-publish-age `{age_text}` is out of range: {e}"))?;
    let lock: Lock = toml::from_str(lock_text).map_err(|e| format!("parsing Cargo.lock: {e}"))?;
    let mut summary = Summary {
        age: age_text.to_owned(),
        ..Summary::default()
    };

    let registry: Vec<&LockPackage> = lock
        .package
        .iter()
        .filter(|p| {
            p.source
                .as_deref()
                .is_some_and(|s| CRATES_IO_SOURCES.contains(&s))
        })
        .collect();
    summary.registry_packages = registry.len();
    summary.other_sources =
        lock.package.iter().filter(|p| p.source.is_some()).count() - registry.len();

    let mut entries: HashMap<&str, Vec<IndexEntry>> = HashMap::new();
    for p in &registry {
        if !entries.contains_key(p.name.as_str()) {
            let body = index
                .get(&p.name)
                .ok_or_else(|| format!("no index data for {}", p.name))?;
            entries.insert(&p.name, parse_index(&p.name, body)?);
        }
    }
    let mut versions: HashMap<&str, Vec<Candidate>> = HashMap::new();
    for (name, list) in &entries {
        let mut out = Vec::new();
        for entry in list {
            let Ok(version) = Version::parse(&entry.vers) else {
                continue;
            };
            let pubtime = entry
                .pubtime
                .as_deref()
                .map(|t| {
                    t.parse::<Timestamp>()
                        .map_err(|e| format!("pubtime of {name} {}: {e}", entry.vers))
                })
                .transpose()?;
            out.push(Candidate {
                version,
                pubtime,
                entry,
            });
        }
        versions.insert(name, out);
    }
    let mut locked: Vec<&Candidate> = Vec::new();
    for p in &registry {
        let c = versions
            .get(p.name.as_str())
            .and_then(|list| list.iter().find(|c| c.entry.vers == p.version))
            .ok_or_else(|| {
                format!(
                    "{} {} from Cargo.lock is not in the crates.io index",
                    p.name, p.version
                )
            })?;
        if c.pubtime.is_none() {
            return Err(format!(
                "{} {} has no pubtime in the index",
                p.name, p.version
            ));
        }
        locked.push(c);
    }
    let young: Vec<usize> = (0..registry.len())
        .filter(|&i| locked[i].pubtime > Some(cutoff))
        .collect();
    summary.young = young.len();

    // A young package that falls back changes the requirements it puts on its own
    // dependencies, so iterate until the chosen fallbacks stop changing.
    let mut effective = locked.clone();
    let mut fallbacks: HashMap<usize, Option<&Candidate>> = HashMap::new();
    let mut reqs = HashMap::new();
    for _ in 0..=young.len() {
        reqs = requirements(&registry, &effective, &locked, workspace_reqs)?;
        let mut changed = false;
        for &i in &young {
            let default_req = vec![(
                VersionReq::parse(&format!("^{}", locked[i].version)).map_err(|e| e.to_string())?,
                "Cargo.lock".to_owned(),
            )];
            let own = reqs.get(&i).unwrap_or(&default_req);
            let fallback = versions[registry[i].name.as_str()]
                .iter()
                .filter(|c| !c.entry.yanked && c.pubtime.is_some_and(|t| t <= cutoff))
                .filter(|c| own.iter().all(|(r, _)| r.matches(&c.version)))
                .max_by(|a, b| a.version.cmp(&b.version));
            let next = fallback.unwrap_or(locked[i]);
            if !std::ptr::eq(effective[i], next) {
                effective[i] = next;
                changed = true;
            }
            fallbacks.insert(i, fallback);
        }
        if !changed {
            break;
        }
    }

    let mut findings = Vec::new();
    for &i in &young {
        let p = registry[i];
        let pubtime = locked[i].pubtime.expect("checked above");
        let line = find_lock_line(lock_text, &p.name, &p.version);
        let published = format!(
            "{} ago ({})",
            days(now.duration_since(pubtime)),
            pubtime.strftime("%Y-%m-%d")
        );
        let mut required_by: Vec<String> = reqs
            .get(&i)
            .map(|r| {
                r.iter()
                    .map(|(r, from)| format!("`{r}` from {from}"))
                    .collect()
            })
            .unwrap_or_default();
        if required_by.len() > 5 {
            let more = required_by.len() - 5;
            required_by.truncate(5);
            required_by.push(format!("{more} more"));
        }
        let (rule, severity, message, help) = match fallbacks[&i] {
            Some(f) => (
                YOUNG,
                Severity::Note,
                format!(
                    "{} {} was published {published}; with min-publish-age \"{age_text}\" it stays because it is locked, but `cargo update` or a fresh resolve would pick {}",
                    p.name, p.version, f.version
                ),
                format!(
                    "no action needed; to take the newest version anyway: CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow cargo update -p {}",
                    p.name
                ),
            ),
            None => {
                summary.unsatisfiable += 1;
                (
                    UNSATISFIABLE,
                    Severity::Error,
                    format!(
                        "{} {} was published {published}, and no version older than \"{age_text}\" satisfies {}; `cargo update` or a fresh resolve would fail",
                        p.name,
                        p.version,
                        required_by.join(", ")
                    ),
                    format!(
                        "wait until {} or allow it explicitly: CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow cargo update -p {}",
                        pubtime
                            .checked_add(age)
                            .map(|t| t.strftime("%Y-%m-%d").to_string())
                            .unwrap_or_default(),
                        p.name
                    ),
                )
            }
        };
        findings.push(Finding {
            rule,
            severity,
            file: "Cargo.lock".to_owned(),
            line,
            column: 1,
            message,
            help,
        });
    }
    Ok((findings, summary))
}

/// Names of crates.io packages in a Cargo.lock, deduplicated.
pub fn registry_names(lock_text: &str) -> Result<Vec<String>, String> {
    let lock: Lock = toml::from_str(lock_text).map_err(|e| format!("parsing Cargo.lock: {e}"))?;
    let mut names: Vec<String> = lock
        .package
        .into_iter()
        .filter(|p| {
            p.source
                .as_deref()
                .is_some_and(|s| CRATES_IO_SOURCES.contains(&s))
        })
        .map(|p| p.name)
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// Full preview for a repository root, fetching from crates.io.
pub fn run(root: &Path, age_text: &str) -> Result<(Vec<Finding>, Summary), String> {
    parse_age(age_text)?;
    let lock_path = root.join("Cargo.lock");
    let lock_text = std::fs::read_to_string(&lock_path).map_err(|e| {
        format!(
            "{}: {e}; the cooldown preview needs a lockfile, run `cargo generate-lockfile` first",
            lock_path.display()
        )
    })?;
    let workspace_reqs = workspace_requirements(root)?;
    let names = registry_names(&lock_text)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        // Large index files (web-sys is ~4.5 MB) can take minutes on a slow link, so only
        // bound the connection and the wait for response headers, not the whole download.
        .timeout_connect(Some(std::time::Duration::from_secs(30)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(60)))
        .build()
        .into();
    let index = fetch_all(&names, &|name| fetch_sparse(&agent, name))?;
    preview(
        &lock_text,
        &workspace_reqs,
        &index,
        age_text,
        Timestamp::now(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages() {
        assert_eq!(
            parse_age("7 days").unwrap(),
            SignedDuration::from_hours(168)
        );
        assert_eq!(
            parse_age("1 week").unwrap(),
            SignedDuration::from_hours(168)
        );
        assert_eq!(parse_age("0").unwrap(), SignedDuration::ZERO);
        assert!(parse_age("7d").is_err());
        assert!(parse_age("seven days").is_err());
    }

    #[test]
    fn sparse_paths() {
        assert_eq!(sparse_path("a"), "1/a");
        assert_eq!(sparse_path("cc"), "2/cc");
        assert_eq!(sparse_path("syn"), "3/s/syn");
        assert_eq!(sparse_path("Serde"), "se/rd/serde");
    }
}
