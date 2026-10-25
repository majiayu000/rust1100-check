# Real-world runs

All numbers below come from runs on 2026-10-22 with the code in this repository. Raw outputs are in [`real-repos/`](real-repos/): `<owner>__<repo>.commit` (the exact commit scanned), `.static.json`, `.cooldown.json` and `.beta.json`.

## 1. Seventeen repositories, static checks

Shallow clones of the default branch. "Verdict" is a manual check of what consumes each flagged value.

| Repository | ★ | Files | Error | Warning | Note | Verdict |
|---|---|---|---|---|---|---|
| [vinezombie/vinezombie](https://github.com/vinezombie/vinezombie) | 10 | 1 | 1 | 0 | 0 | **True positive, broken today.** `toolchain: 1.70` goes to actions-rs/toolchain, which passes `1.7` to rustup → Rust 1.7.0. The workflow has 0 recorded runs, so it was never exercised |
| [SolarLiner/interflow](https://github.com/SolarLiner/interflow) | 64 | 5 | 0 | 7 | 0 | True positive. `toolchain: 1.88`/`1.96` and `rust: [1.93, 1.94, 1.95, 1.96]` all go to actions-rs/toolchain; bumping any of them to 1.100 installs 1.1.0 |
| [kdl-org/kdl-rs](https://github.com/kdl-org/kdl-rs) | 523 | 3 | 0 | 1 | 0 | True positive. `rust: [1.95, stable]` goes to actions-rs/toolchain |
| [nanoqsh/tauri-wasm](https://github.com/nanoqsh/tauri-wasm) | 13 | 3 | 0 | 1 | 0 | Latent. `MSRV: 1.85` in workflow `env:` is not referenced anywhere in the repo, so nothing breaks today; kept as a warning because any step's process can read it |
| [zino-rs/zino](https://github.com/zino-rs/zino) | 1,155 | 4 | 0 | 0 | 1 | Correct note. `toolchain: 1.90` is loaded as 1.9, and dtolnay/rust-toolchain maps it back to 1.90 |
| [Janus-admin/janus](https://github.com/Janus-admin/janus) | 1 | 18 | 0 | 0 | 3 | Correct notes. Three `toolchain: 1.80` inputs to dtolnay/rust-toolchain |
| [goedelsoup/gensonnet-rs](https://github.com/goedelsoup/gensonnet-rs) | 0 | 12 | 0 | 0 | 2 | Correct notes. `rust: [1.70, stable]` and an `include` entry, used only as dtolnay's `toolchain` and in `name:` |
| [aws/aws-nitro-enclaves-nsm-api](https://github.com/aws/aws-nitro-enclaves-nsm-api) | 87 | 7 | 0 | 0 | 0 | Correctly silent. `rust: [1.92, stable, nightly]` is only used by dtolnay's `toolchain` and the job `name` |
| [tokio-rs/tokio](https://github.com/tokio-rs/tokio) | 33,478 | 11 | 0 | 0 | 0 | Correctly silent after a fix: `env: rust_stable: 1.85` in `stress-test.yml` is only used by dtolnay's `toolchain`. The first version of the tool reported it (see section 4) |
| [BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep) | 69,327 | 8 | 0 | 0 | 0 | No unquoted versions |
| [clap-rs/clap](https://github.com/clap-rs/clap) | 16,788 | 31 | 0 | 0 | 0 | No unquoted versions |
| [serde-rs/serde](https://github.com/serde-rs/serde) | 10,896 | 6 | 0 | 0 | 0 | Uses three-part versions (`1.56.0`) |
| [dtolnay/anyhow](https://github.com/dtolnay/anyhow) | 6,685 | 3 | 0 | 0 | 0 | Uses three-part versions |
| [rust-ndarray/ndarray](https://github.com/rust-ndarray/ndarray) | 4,332 | 7 | 0 | 0 | 0 | Uses three-part versions |
| [programatik29/axum-server](https://github.com/programatik29/axum-server) | 264 | 2 | 0 | 0 | 0 | Uses `dtolnay/rust-toolchain@1.82` (a ref is always a string) |
| [majiayu000/litellm-rs](https://github.com/majiayu000/litellm-rs) | 121 | 60 | 0 | 0 | 0 | — |
| [majiayu000/jsonrepair-rs](https://github.com/majiayu000/jsonrepair-rs) | 4 | 7 | 0 | 0 | 0 | — |

Totals: 17 repositories, 188 files, 1 error, 9 warnings, 6 notes. 10 of 17 have no findings. After the fixes in section 4: 0 false positives among the 16 findings (10 true or latent, 6 correct notes). The repositories were chosen partly from code search hits for unquoted versions, so this hit rate overstates what a random Rust repository would show: none of the 7 large, well-known repositories (ripgrep, tokio, clap, serde, anyhow, ndarray, zino) has an error or warning.

## 2. Cooldown preview, `--min-publish-age "7 days"`

Run 2026-10-22 against each repository's Cargo.lock. Repositories without a committed lockfile got one from `cargo generate-lockfile` (Rust 1.99.0) the same day, which is the situation of a CI job that resolves fresh.

| Repository | Lockfile | crates.io packages | Younger than 7 days | Would fail to resolve |
|---|---|---|---|---|
| zino-rs/zino | generated | 1,145 | 70 | 0 |
| goedelsoup/gensonnet-rs | generated | 460 | 18 | 0 |
| vinezombie/vinezombie | generated | 87 | 7 | 0 |
| programatik29/axum-server | generated | 118 | 6 | 0 |
| tokio-rs/tokio | generated | 194 | 3 | 0 |
| aws/aws-nitro-enclaves-nsm-api | generated | 44 | 1 | 0 |
| dtolnay/anyhow | generated | 39 | 0 | 0 |
| serde-rs/serde | generated | 30 | 0 | 0 |
| clap-rs/clap | committed | 464 | 0 | 0 |
| majiayu000/litellm-rs | committed | 641 | 0 | 0 |
| Janus-admin/janus | committed | 666 | 0 | 0 |
| nanoqsh/tauri-wasm | committed | 452 | 0 | 0 |
| rust-ndarray/ndarray | committed | 159 | 0 | 0 |
| kdl-org/kdl-rs | committed | 151 | 0 | 0 |
| SolarLiner/interflow | committed | 147 | 0 | 0 |
| majiayu000/jsonrepair-rs | committed | 62 | 0 | 0 |
| BurntSushi/ripgrep | committed | 52 | 0 | 0 |

What this shows: a committed lockfile older than the cooldown is untouched. A fresh resolve today pulls in a handful to dozens of versions published in the last week (tokio 1.54.1 was 7 hours old in zino's resolve; the preview says a cooldown would pick 1.53.3). No repository would fail to resolve with a 7-day cooldown on this date. Spot checks against the index: libc 0.2.191 (2026-10-16) → 0.2.190 (2026-10-02); tokio 1.54.1 → 1.53.3 (1.54.0 is yanked).

On this repository's own lockfile with `"14 days"`, 4 packages are unsatisfiable, all for a real reason: `Cargo.toml` asks for `ureq = "3.5"`, and ureq 3.5.0 was 9 hours old.

## 3. Beta diff, `--beta-diff --baseline 1.99.0`

Beta was `rustc 1.100.0-beta.7 (fd8e211ae 2026-10-17)`.

| Repository | New diagnostics with beta |
|---|---|
| majiayu000/litellm-rs | 4 warnings, all from the newly stabilized Cargo lints: `package.homepage` is redundant with `package.repository` (Cargo.toml:9), unused dependencies `lru` (:97), `dirs` (:132), `sysinfo` (:137) |
| majiayu000/jsonrepair-rs | 0 |
| kdl-org/kdl-rs | 0 |
| dtolnay/anyhow | 0 |
| serde-rs/serde | 0 |
| nanoqsh/tauri-wasm | 0 |

The litellm-rs warnings matter for any CI that runs with `-D warnings` (or setup-rust-toolchain's default `build-warnings: deny`).

## 4. Code search samples for the script rules

To find real script samples, 7 GitHub code searches were run (`"rustc --version" "bc"`, `"rustc -V" "bc"`, `"rustc --version" "awk" "<"`, `"RUST_VERSION" "[[" "<" language:Shell`, `"rustc_version" "float("`, `"MSRV" "sort" language:Shell`, `"rustc" "1\.[0-9]{2}"`). The top results gave 241 files in 229 repositories, which were downloaded and scanned.

- True positives (2 files): Poudlardo/Allocine2Letterboxd `build.sh` has `[[ "$RUST_VERSION" < "1.70.0" ]]`, a string comparison that says 1.100.0 is older than 1.70.0. Monles/rust-malware-re-comparison extracts versions with `grep -oE '1\.[0-9]{2}\.[0-9]+'` (3 lines), which skips 1.100.x. Both are fixtures now.
- The first version of R3 also reported 13 lines in 7 files that were false positives: a `<<<` here-string and a `>/dev/null` redirect read as awk comparisons; `cut -c 7-` and `cut -c 3-` (open-ended, harmless) read as fixed-width cuts; `sort -u` used only to de-duplicate; and a Python keyword argument `commit=commit, rustc=rustc_version()` taken as "variable assigned from rustc", which then flagged seven `commit[:12]` slices. All four causes were fixed and are covered by `script::tests::real_world_false_positives_stay_quiet`. These were found and fixed on the same sample, so 0 false positives here is an in-sample number.
- A grep over the same 241 files for `| bc`, awk comparisons, `[[ … < … ]]` and `float(` near a version word found no Rust-version comparison the rules missed.

Separately, code search for unquoted trailing-zero versions in workflows (`"toolchain: 1.80"`, `"toolchain: 1.70"`, `"toolchain: 1.90"`, `"rust: 1.70"`, `"msrv: 1.80"`) gave these consumers of an unquoted `1.x0`: actions-rs/toolchain in vinezombie/vinezombie and ranajahanzaib/rico (broken today), and dtolnay/rust-toolchain in zino-rs/zino, Janus-admin/janus, goedelsoup/gensonnet-rs (mitigated).

Fixtures in `tests/fixtures/` are minimized from: SolarLiner/interflow, vinezombie/vinezombie, aws/aws-nitro-enclaves-nsm-api, zino-rs/zino, nanoqsh/tauri-wasm, Poudlardo/Allocine2Letterboxd, Monles/rust-malware-re-comparison; the env case in `yaml::tests` is from tokio-rs/tokio.

## 5. New rule `i686-windows-host-toolchain` (added 2026-10-25)

Added after the runs above. Re-running the 17 repositories in section 1 with it gives the same counts: none of them uses a 32-bit Windows host toolchain.

To test it, 5 GitHub code searches (`"stable-i686-pc-windows" path:.github/workflows` 91 files, `"nightly-i686-pc-windows" path:.github/workflows` 13, `"toolchain: stable-i686"` 65, `"i686-pc-windows" path:rust-toolchain.toml` 0, `"i686-pc-windows-msvc" "toolchain" path:.github/workflows` 3,408) gave 143 distinct files in 134 repositories, scanned as downloaded:

- Reported: 118 lines in 69 repositories. Every reported line was read by hand. All were host toolchains: `toolchain: stable-i686-pc-windows-msvc` (23 lines), `rust: stable-i686-pc-windows-gnu`-style matrix values, `echo "stable-i686-pc-windows-msvc" > rust-toolchain`, `rustup default/install/run/override`, `cargo +nightly-i686-pc-windows-msvc build`, `rustup-init --default-host=i686-pc-windows-msvc`, and names assembled from the matrix (bastion-rs/bastion and bastion-rs/agnostik `${{ matrix.version }}-${{ matrix.toolchain }}`, blckngm/webview2 `${{ matrix.version }}-${{ matrix.arch }}-pc-windows-${{ matrix.env }}`, racer-rust/racer `rustup set default-host ${{ matrix.toolchain }}`).
- False positives: 0 after one fix (a step `name:` that echoed the matrix toolchain was reported next to the real install step; `name:` is no longer expanded).
- Not reported: the other 65 repositories. A grep of their files for `<channel>-i686-pc-windows-*` and `default-host` found such names only in files GitHub does not run (vendored subprojects' `.github/workflows`, a README, a build log) or on commented-out lines; everything else mentions the triple only as a cross-compile target (`target:`, `targets:`, `rustup target add`, `--target`).

Fixtures: `tests/fixtures/i686-host` (minimized from bastion-rs/bastion, stanislav-tkach/os_info, racer-rust/racer, alisawuffles/tokenizers-superbpe, llnut/dnf-login) and `tests/fixtures/i686-cross` (cross-compiling forms and a pre-1.100 pin, none reported).

