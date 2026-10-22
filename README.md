# rust1100-check

[![CI](https://github.com/majiayu000/rust1100-check/actions/workflows/ci.yml/badge.svg)](https://github.com/majiayu000/rust1100-check/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/majiayu000/rust1100-check)](https://github.com/majiayu000/rust1100-check/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV 1.88](https://img.shields.io/badge/MSRV-1.88-orange.svg)](Cargo.toml)

**Rust 1.100 readiness check for your CI:** finds unquoted versions in GitHub Actions / GitLab CI YAML that `rust-toolchain` setups read as Rust 1.1, MSRV scripts that compare `rustc --version` as strings, new lints on the 1.100 beta, and previews which `Cargo.lock` dependencies Cargo's new `min-publish-age` cooldown would hold back.

Rust 1.100 (2026-11-12) is the first release with a three-digit minor version. Unquoted in YAML, `1.100` is the number 1.1, and rustup installs Rust 1.1.0 from 2015 on a Linux runner. The same thing already happens to `1.70`, `1.80` and `1.90` (read as 1.7, 1.8, 1.9). Scripts that compare versions as strings put `1.100` before `1.99`. The release also stabilizes Cargo lints, changes never-type fallback, and adds `registry.global-min-publish-age`.

`cargo rust1100-check` is read-only unless you pass `--fix`.

## Install

Prebuilt binaries for Linux x86_64 and macOS (arm64, x86_64) are on the [Releases page](https://github.com/majiayu000/rust1100-check/releases). Or build it (Rust 1.88+):

```bash
cargo install --git https://github.com/majiayu000/rust1100-check --locked
```

## Usage

```bash
cargo rust1100-check
```

```bash
cargo rust1100-check --fix
```

```bash
cargo rust1100-check --min-publish-age "7 days"
```

```bash
cargo rust1100-check --beta-diff
```

| Flag | What it does |
|---|---|
| `[PATH]` | Repository root, default `.` |
| `--fix` | Quote the flagged YAML versions in place (`1.90` → `"1.90"`), then report what is left |
| `--format text\|json\|github\|sarif` | Output format, default `text`. `github` prints workflow annotations |
| `--min-publish-age "<N> days"` | Preview Cargo's cooldown against `Cargo.lock` (reads `pubtime` from the crates.io sparse index) |
| `--beta-diff` | Run `cargo check` with `--baseline` (default `stable`) and with `beta`, report diagnostics only beta emits. Until Nov 12, beta is 1.100 |
| `--fail-on error\|warning\|note\|never` | Lowest severity that sets exit code 1, default `error` |

To silence one finding, put `rust1100-check: ignore` in a comment on the same line or the line above.

Exit codes: `0` nothing at or above `--fail-on`, `1` findings, `2` the tool itself failed (bad arguments, network error, missing toolchain).

## How it compares

| | rust1100-check | actionlint | zizmor | yamllint | dtolnay/rust-toolchain |
|---|---|---|---|---|---|
| Unquoted version read as a float (`1.100` → 1.1) | ✅ | — | — | ⚠️ quotes everything | ⚠️ fixes its own `toolchain` input only |
| Knows when dtolnay/rust-toolchain already handles it | ✅ | — | — | — | — |
| GitLab CI, CircleCI, Travis, compose | ✅ | — | — | ✅ | — |
| Version compared as string in scripts | ✅ | — | — | — | — |
| stable vs 1.100 beta diagnostics | ✅ | — | — | — | — |
| Cargo `min-publish-age` cooldown preview | ✅ | — | — | — | — |

Use it next to actionlint and zizmor, not instead of them. Full table with cargo-msrv, cargo-deny and the cooldown tools: [docs/comparison.md](docs/comparison.md).

## Example output

Real output on [vinezombie/vinezombie](https://github.com/vinezombie/vinezombie), whose MSRV job asks actions-rs/toolchain for `1.70`:

```text
.github/workflows/ci.yml
  20:22  error    yaml-float-version-truncated
          `toolchain: 1.70` is an unquoted YAML number and is loaded as 1.7; rustup would install Rust 1.7.0
          help: quote it: `toolchain: "1.70"`, or use a full version such as `1.70.0`

files checked: 1; errors: 1, warnings: 0, notes: 0
```

Cooldown preview on this repository (Cargo.lock of 2026-10-22, `--min-publish-age "14 days"`), shortened:

```text
Cargo.lock
  ...  note     cooldown-young-locked
          libc 0.2.191 was published 5 days ago (2026-10-16); with min-publish-age "14 days" it stays because it is locked, but `cargo update` or a fresh resolve would pick 0.2.190
  ...  error    cooldown-unsatisfiable
          ureq 3.5.0 was published 9 hours ago (2026-10-21), and no version older than "14 days" satisfies `^3.5` from rust1100-check; `cargo update` or a fresh resolve would fail

cooldown preview (min-publish-age "14 days"): 111 crates.io packages, 18 younger than the age, 4 would fail to resolve; 0 packages from other sources not checked
to enable it (Cargo 1.100+), add to .cargo/config.toml:

[registry]
global-min-publish-age = "14 days"
```

GitHub annotation format (`--format github`) on [kdl-org/kdl-rs](https://github.com/kdl-org/kdl-rs):

```text
::warning file=.github/workflows/ci.yml,line=31,col=16,title=rust1100-check/yaml-float-rust-version::`rust: 1.95` is an unquoted YAML number; it works today, but bumping it to 1.100 loads as 1.1 and installs Rust 1.1.0%0Ahelp: quote it: `rust: "1.95"`, or use a full version such as `1.95.0`
```

Results on 17 real repositories, including the ones with zero findings, are in [examples/README.md](examples/README.md).

## Rules

| Rule | Severity | Finds | Fix |
|---|---|---|---|
| `yaml-float-version-truncated` | error (Rust key), warning (other `*version*` key), note (handled by dtolnay/rust-toolchain) | Unquoted `1.90`, `1.100`, `3.10` in CI YAML: loaded as 1.9, 1.1, 3.1 | Quote it, or write `1.90.0` |
| `yaml-float-rust-version` | warning | Unquoted `rust: [1.92]`, `MSRV: 1.85`: fine today, becomes 1.1 when bumped to 1.100 | Quote it now |
| `version-compare-as-string` | warning | `[[ "$v" < "1.70" ]]`, `\| bc`, `float(`, `sort` without `-V`, `cut -c1-4` on a Rust version | Compare the minor as an integer, or `sort -V` |
| `two-digit-minor-pattern` | warning | `1\.[0-9]{2}`, `1\.\d\d` | `1\.[0-9]+` |
| `cooldown-unsatisfiable` | error | With `--min-publish-age`, a requirement only versions newer than the age can satisfy | Wait, or `CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow cargo update -p <crate>` |
| `cooldown-young-locked` | note | Locked version newer than the age: kept, but `cargo update` would pick an older one | Nothing, unless you want the newest |
| `beta-new-diagnostic` | follows the diagnostic | With `--beta-diff`, a warning or error beta reports and the baseline does not | See the [1.100 release notes](https://github.com/rust-lang/rust/issues/163811) |
| `unparsable-yaml` | warning | A CI YAML file that could not be parsed and so was not checked | Fix the YAML |

What is scanned: `.github/**/*.yml`, `action.yml` anywhere, `.gitlab-ci.yml`, `.circleci/config.yml`, `.travis.yml`, `azure-pipelines.yml`, `.cirrus.yml`, `bitbucket-pipelines.yml`, compose files; shell, Makefile, justfile, Python, JS/TS and `build.rs`, plus `run:`/`script:` blocks inside CI YAML. `.gitignore` is respected.

### Why dtolnay/rust-toolchain is a note

[dtolnay/rust-toolchain](https://github.com/dtolnay/rust-toolchain) maps a two-part input like `1.1` back to the release it must have meant (`1.100` once 1.100 is out, `1.10` before), so its `toolchain` input survives the YAML float. rust1100-check follows where each `matrix.*` and `env.*` value is used: if every use is that input (or a `name:`), the finding is a note, or is dropped for values that are fine today. If the same value also reaches `run: rustup toolchain install ${{ matrix.rust }}`, `cargo +${{ matrix.rust }}`, a cache key or another action such as actions-rust-lang/setup-rust-toolchain or actions-rs/toolchain, it stays an error.

### How the cooldown preview works

Cargo 1.100 ignores versions whose index `pubtime` is newer than `global-min-publish-age`, **unless they are already in Cargo.lock** ([rust-lang/cargo#17335](https://github.com/rust-lang/cargo/pull/17335)). So turning it on does not break an existing lockfile. It changes what `cargo update`, a new dependency, or a CI job without a committed lockfile resolves to. The preview reads Cargo.lock, fetches each crates.io package's index file, and for every locked version younger than the age finds the newest old-enough, non-yanked version that satisfies every requirement on it. A young parent's requirements are replaced by those of its own fallback, repeated until stable. It is a per-package approximation, not a full re-resolve. Git, path and other-registry packages are counted and skipped. A missing Cargo.lock or missing `pubtime` is an error, not a silent pass.

## GitHub Action

```yaml
- uses: actions/checkout@v7
- uses: majiayu000/rust1100-check@v0.1.0
  with:
    min-publish-age: "7 days"   # optional
    fail-on: error              # default
```

The action downloads the release binary for the runner (falls back to `cargo install --git` on other platforms) and runs it with `--format github`, so findings show up as annotations on the PR. For code scanning, run `cargo rust1100-check --format sarif > rust1100.sarif` and upload it with `github/codeql-action/upload-sarif`.

## Docs

- [docs/research.md](docs/research.md): what changes in 1.100, how CI actions handle the version input, competing tools
- [docs/design.md](docs/design.md): rules, false-positive controls, output formats
- [docs/comparison.md](docs/comparison.md): feature comparison with adjacent tools

## License

[MIT](LICENSE)
