//! R7: 32-bit Windows used as the *host* toolchain. From Rust 1.100, i686-pc-windows-msvc and
//! i686-pc-windows-gnu ship std only, no rustc/cargo
//! (https://blog.rust-lang.org/2026/10/02/demoting-i686-windows-targets-to-std-only/).
//! Cross-compiling with `--target i686-pc-windows-*` keeps working and is not reported.

use std::sync::LazyLock;

use regex::Regex;

use crate::{Finding, Severity};

pub const HOST_TOOLCHAIN: &str = "i686-windows-host-toolchain";

const BLOG: &str =
    "https://blog.rust-lang.org/2026/10/02/demoting-i686-windows-targets-to-std-only/";

/// A rustup toolchain name with an i686 Windows host: `stable-i686-pc-windows-msvc`,
/// `nightly-2026-09-01-i686-pc-windows-gnu`, `1.100.0-i686-pc-windows-msvc`. A bare
/// `i686-pc-windows-msvc` is a target triple and does not match.
static TOOLCHAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:^|[^A-Za-z0-9_.-])((stable|beta|nightly(?:-(\d{4}-\d{2}-\d{2}))?|1\.(\d+)(?:\.\d+)?)-i686-pc-windows-(msvc|gnu))\b",
    )
    .unwrap()
});
/// `rustup set default-host i686-pc-windows-msvc`, `rustup-init --default-host i686-…`.
static DEFAULT_HOST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bdefault-host[\s=]+(i686-pc-windows-(msvc|gnu))\b").unwrap());

/// First nightly without i686 Windows host tools, found by reading the dist manifests
/// (`static.rust-lang.org/dist/<date>/channel-rust-nightly.toml`).
fn first_nightly_without_host(env: &str) -> &'static str {
    if env == "gnu" {
        "2026-08-27"
    } else {
        "2026-09-10"
    }
}

/// Why this toolchain is affected, or `None` when it still has host tools.
fn affected(channel: &str, date: Option<&str>, minor: Option<u32>, env: &str) -> Option<String> {
    let cutoff = first_nightly_without_host(env);
    if let Some(d) = date {
        return (d >= cutoff)
            .then(|| format!("nightlies from {cutoff} on ship no rustc/cargo for it"));
    }
    match channel {
        "stable" => Some(
            "stable stops shipping rustc/cargo for it with Rust 1.100 on 2026-11-12".to_owned(),
        ),
        "beta" => Some("beta (1.100) already ships no rustc/cargo for it".to_owned()),
        "nightly" => Some(format!(
            "nightly has shipped no rustc/cargo for it since nightly-{cutoff}"
        )),
        _ => minor
            .filter(|m| *m >= 100)
            .map(|_| "Rust 1.100 and later ship no rustc/cargo for it".to_owned()),
    }
}

fn help(env: &str) -> String {
    format!(
        "use an x86_64 host toolchain and cross-compile: `rustup target add i686-pc-windows-{env}` \
         (or `targets: i686-pc-windows-{env}` on the toolchain action) and `cargo build --target i686-pc-windows-{env}`; see {BLOG}"
    )
}

/// Check one line of text. `via` names the template the line was expanded from, if any.
pub fn check_line(file: &str, text: &str, line: usize, via: Option<&str>, out: &mut Vec<Finding>) {
    let suffix = via
        .map(|t| format!(" (from `{}` with matrix values filled in)", t.trim()))
        .unwrap_or_default();
    for c in TOOLCHAIN.captures_iter(text) {
        let name = c.get(1).unwrap();
        let channel = &c[2];
        let minor = c.get(4).and_then(|m| m.as_str().parse().ok());
        let env = &c[5];
        let Some(why) = affected(channel, c.get(3).map(|m| m.as_str()), minor, env) else {
            continue;
        };
        out.push(Finding {
            rule: HOST_TOOLCHAIN,
            severity: Severity::Error,
            file: file.to_owned(),
            line,
            column: if via.is_some() { 1 } else { name.start() + 1 },
            message: format!(
                "`{}` is a 32-bit Windows host toolchain; {why}, so installing or running it fails{suffix}",
                name.as_str()
            ),
            help: help(env),
        });
    }
    for c in DEFAULT_HOST.captures_iter(text) {
        let triple = c.get(1).unwrap();
        out.push(Finding {
            rule: HOST_TOOLCHAIN,
            severity: Severity::Error,
            file: file.to_owned(),
            line,
            column: if via.is_some() { 1 } else { triple.start() + 1 },
            message: format!(
                "default host `{}` makes rustup install 32-bit Windows host toolchains, which Rust 1.100 no longer ships{suffix}",
                triple.as_str()
            ),
            help: help(&c[2]),
        });
    }
}

/// The toolchain names and `default-host` settings that `text` mentions.
pub fn mentions(text: &str) -> Vec<&str> {
    let names = TOOLCHAIN
        .captures_iter(text)
        .map(|c| c.get(1).unwrap().as_str());
    names
        .chain(DEFAULT_HOST.find_iter(text).map(|m| m.as_str()))
        .collect()
}

/// Check every non-comment line of a file.
pub fn check(file: &str, text: &str, out: &mut Vec<Finding>) {
    for (idx, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with('#') || t.starts_with("//") {
            continue;
        }
        check_line(file, line, idx + 1, None, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(text: &str) -> Vec<usize> {
        let mut out = Vec::new();
        check("ci.yml", text, &mut out);
        out.iter().map(|f| f.line).collect()
    }

    #[test]
    fn host_toolchain_forms() {
        let text = "\
rustup toolchain install stable-i686-pc-windows-msvc --no-self-update
toolchain: stable-i686-pc-windows-gnu
rust: nightly-i686-pc-windows-msvc
run: echo \"beta-i686-pc-windows-msvc\" > rust-toolchain
cargo +stable-i686-pc-windows-gnu build --release
channel = \"1.100.0-i686-pc-windows-msvc\"
rustup set default-host i686-pc-windows-msvc
toolchain: nightly-2026-09-10-i686-pc-windows-msvc
";
        assert_eq!(hits(text), (1..=8).collect::<Vec<_>>());
    }

    #[test]
    fn cross_compiling_and_old_pins_are_fine() {
        let text = "\
target: i686-pc-windows-msvc
targets: x86_64-pc-windows-msvc, i686-pc-windows-msvc
rustup target add i686-pc-windows-msvc
cargo build --release --target i686-pc-windows-gnu
args: --target=i686-pc-windows-msvc
path: target/i686-pc-windows-msvc/release/app.exe
toolchain: 1.85.0-i686-pc-windows-msvc
toolchain: nightly-2026-09-09-i686-pc-windows-msvc
toolchain: nightly-2026-08-26-i686-pc-windows-gnu
toolchain: stable-x86_64-pc-windows-msvc
# toolchain: stable-i686-pc-windows-msvc
";
        assert!(hits(text).is_empty(), "{:?}", hits(text));
    }
}
