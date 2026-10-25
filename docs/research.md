# Rust 1.100 调研

数据取自 2026-10-22。所有结论都标了来源；没查到的写“未核实”。

## 1. 发布事实

- 官方 draft release notes（rust-lang/rust#163811，10-05 创建，仍为 open）标题是 `Version 1.100.0 (2026-11-12)`。
- 当前 beta：`rustup toolchain install beta` 实测装到 `1.100.0-beta.7 (fd8e211ae 2026-10-17)`；channel manifest 日期 2026-10-18。
- 本机 stable 1.99.0；dtolnay/rust-toolchain 已经预建好 `1.100`…`1.120` 的分支（`git/matching-refs/heads/1.`）。

## 2. 可能造成破坏的点

按“会不会在用户毫无改动的情况下让 CI 变红/变错”排序。

### 2.1 版本号三位数（本工具的核心）

| 问题 | 实查结论 | 证据 |
|---|---|---|
| YAML 把未加引号的 `1.100` 解析成浮点 1.1 | Ruby YAML 实测：`a: 1.100` → `1.1`；`1.90` → `1.9`；`1.95` → `1.95`；`1.100.0` → `"1.100.0"`（三段版本号是字符串，安全）；`"1.100"` → `"1.100"` | 本机 `ruby -ryaml`；Python 3.10 的同类事故 actions/setup-python#160、#585、#695 |
| **末尾是 0 的两段版本现在就已经错了** | `toolchain: 1.90`、`rust: [1.70]` 今天就会被读成 1.9 / 1.7。这不是 1.100 才有的问题，只是 1.100 会让它重新暴露 | 同上 |
| rustup 收到 `1.1` 会装 Rust 1.1.0 | 实测：`rustup toolchain install 1.1-x86_64-unknown-linux-gnu --force-non-host` 成功装上 `1.1.0-x86_64-unknown-linux-gnu`（ELF x86-64）。macOS arm64 没有 1.1.0 构建，会 404 报错 | 本机 scratch `RUSTUP_HOME` 实测 |
| 按字符串比较版本 | `"1.100" < "1.99"`。已有仓库在 10-01 修过：argumentcomputer/ci-workflows#100 原话 “compared versions as strings, which orders 1.100 before 1.99” | PR 文本 |
| `bc`/`awk` 按浮点比较 | `1.100 >= 1.70` 在浮点下为假 | 推导，未找到公开事故 |
| 只认两位 minor 的正则 / 定宽截取 | `1\.[0-9]{2}`、`cut -c1-4` 会把 1.100 截成 1.10 | 推导；code search 对正则字面量分词不可靠，`"1\.[0-9][0-9]"` 0 命中 |

### 2.2 CI action 对版本输入的处理

| Action | star | 对 `1.1` 的处理 | 结论 |
|---|---|---|---|
| dtolnay/rust-toolchain | 1,592 | `action.yml` 的 parse 步骤有专门的启发式：输入匹配 `^1\.[0-9]+$` 时，按“自 Rust 1.0 起经过的 6 周周期数”还原被 YAML 截掉的 0：`i + 9*i*(10*i<=c) + 90*i*(100*i<=c)`。在 1.100 发布后，`1.1` 会被还原成 `1.100`，`1.9` 还原成 `1.90` | **已缓解**，但只对 `with: toolchain:` 输入生效；同一个 matrix 值如果还拿去 `rustup toolchain install ${{ matrix.rust }}`、cache key、`cargo +${{ matrix.rust }}`，仍然是 1.1 |
| actions-rust-lang/setup-rust-toolchain | 407 | 原样 `rustup toolchain install ${_srt_toolchain//,/ }` | **未缓解**，会装 1.1.0 |
| actions-rs/toolchain | 605 | 2023 年已归档；原样传给 rustup | **未缓解** |
| moonrepo/setup-rust | 135 | 未逐行核实，代码里没看到类似启发式 | 视为未缓解 |
| `run: rustup toolchain install ${{ matrix.rust }}` | — | 原样 | **未缓解** |
| `uses: dtolnay/rust-toolchain@1.100` | — | `uses:` 的 ref 永远是字符串 | 安全 |

### 2.3 rust-toolchain 文件

- `rust-toolchain.toml` 里 `channel = 1.100` 是 TOML 浮点，rustup 直接解析失败，今天写 `1.95` 也一样失败，所以现实里不存在“悄悄装错”的情况。**不做规则**，只在设计里说明。
- 旧式纯文本 `rust-toolchain` 文件没有类型，`1.100` 就是 `1.100`。安全。
- `Cargo.toml` 的 `package.rust-version = "1.100"` 必须是字符串，安全。

### 2.4 1.100 本身的语言/工具变化（draft release notes 原文归纳）

可能让现有代码在 1.100 上编译失败或出新 warning 的：

- never type 稳定，**never type fallback 在所有 edition 上改为 `!`**（rust-lang/rust#155499，标为 breaking change，有 crater 分析）。同一个 PR 移除了 `dependency_on_unit_never_type_fallback` lint，所以“在 1.99 stable 上会触发这个 lint 的代码”正是会在 1.100 上坏掉的代码。
- `Infallible` 变成 `!` 的类型别名。
- 重复 `#[repr(..)]` 新 lint（#157036）；`repr(packed)` 里含 `repr(align)` 字段从硬错误降为 deny-by-default lint `aligned_fields_in_packed`（#162160，只对 `repr(C)`）。
- `box` 模式不再允许（#156749）；trait object 一律要求 dyn-compatible（#154992）；`if let` 中闭包捕获更精确可能出新错误（#154210）；`Box` 上部分 impl 泛化到任意 allocator 可能导致新的类型推断错误（#161946）；`ambiguous_glob_imported_traits` 覆盖范围扩大（#161805）。
- ARM 上会改 ABI 的 `-Ctarget-feature`/`-Ctarget-cpu` 变成硬错误（#161280）。
- rustdoc 新 lint `invalid_markdown_table`（#159583）。
- **Cargo lints 稳定**（rust-lang/cargo#17298）：`[lints.cargo]` 生效。默认 warn 的有 `blanket_hint_mostly_unused`、`missing_lints_inheritance`、`redundant_homepage`、`unknown_lints`、`unused_dependencies`、`unused_workspace_dependencies`、`unused_workspace_package_fields`；默认 deny 的有 `text_direction_codepoint_in_comment`、`text_direction_codepoint_in_literal`。`package.rust-version` 低于 1.79 时默认级别的 lint 被忽略。pragma-org/amaru#1329 升 1.100 时说这些 lint “took some effort to adhere to”。对 `RUSTFLAGS=-D warnings` 或 setup-rust-toolchain 默认 `build-warnings: deny` 的 CI，新 warning 就是红灯。
- `cargo install` 默认使用包内 lockfile（cargo#17388），`--locked` 变成 no-op。
- `cargo update --breaking`（unstable）被移除（cargo#17333）。
- build-dir layout v2 重新稳定（cargo#17354），依赖 `target/` 内部布局的脚本可能要改。

这些变化**无法静态可靠判断**，所以工具用“stable 与 beta 各跑一次 `cargo check`，只报 beta 新增的诊断”来覆盖（见设计 R6）。

### 2.5 Cargo 冷却期（min-publish-age）

来源：rust-lang/cargo#17335（08-28 合并），以及 cargo 文档 `config.md`、`resolver.md`、`registry-index.md`。

- 配置：`[registry] global-min-publish-age = "7 days"`；按 registry 覆盖 `[registries.<name>] min-publish-age`；取值 “整数 + seconds/minutes/hours/days/weeks/months”，或 `"0"`。**默认不开启**。
- 行为开关：`[resolver] incompatible-publish-age = "deny"`（默认 deny），环境变量 `CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow` 用于强推紧急更新，结果会保留在 lockfile 中。
- 时间来源：index 条目的 **`pubtime`** 字段（原始发布时间，不随 yank 变化）。实查 `https://index.crates.io/se/rd/serde` 316 行全部带 `pubtime`。registry 不支持时忽略。
- **关键语义**：deny 时“忽略 pubtime 不满足的版本，**除非它已经在 Cargo.lock 里**”。所以开启冷却期不会让现有 lockfile 立刻失败，影响出现在：
  1. 没有提交 Cargo.lock 的库（CI 每次重新解析）；
  2. `cargo update` / `cargo generate-lockfile` / 新增依赖；
  3. 某个依赖的版本要求只有冷却期内的版本能满足 → 解析直接报错。
- 不影响 `cargo install`（resolver 配置本来就不作用于 install），也不管 git/path 依赖。
- 需要 Cargo 1.100+ 才会读这些配置（文档 “MSRV: Respected as of 1.100+”）。

### 2.6 i686 Windows 不再提供主机工具链（2026-10-25 补充）

- 官方原文（blog.rust-lang.org，2026-10-02，Mateusz Mikuła、Ralf Jung 代表 Compiler team）：“With Rust 1.100.0 … host tools such as the compiler will be no longer available”，`i686-pc-windows-msvc` 从 Tier 1 with host tools 降为 Tier 1 without host tools，`i686-pc-windows-gnu` 从 Tier 2 with host tools 降为 Tier 2 without host tools；标准库继续发布；32 位 Windows 二进制需要从受支持的主机交叉编译。依据 RFC 3999（msvc）和 MCP 1020（gnu）。
- draft release notes（#163811）只列出了 `-gnu` 的降级（#161681），`-msvc` 没有单列；以博客为准。
- dist manifest 实查（10-25）：stable 1.99.1 两个三元组的 rustc、cargo 都在；beta 1.100.0-beta.7 和 nightly 1.101.0 的 rustc、cargo 都不可用，rust-std 可用。nightly 按日期二分：`-gnu` 最后一个有主机工具的是 2026-08-26，`-msvc` 是 2026-09-09。
- GitHub code search：`"stable-i686-pc-windows" path:.github/workflows` 91 个文件，`"nightly-i686-pc-windows" path:.github/workflows` 13 个，`"toolchain: stable-i686"` 65 个；`"i686-pc-windows-msvc" "toolchain" path:.github/workflows` 3,408 个，但绝大多数是 `target:` 交叉编译写法，不受影响。

## 3. 竞品

`gh api search/repositories`，按 star 排序取前 3，拆词 11 组：

| 查询 | total | 最相关结果 | 结论 |
|---|---|---|---|
| `rust 1.100` | 27 | 全是 guessing game（1–100） | 噪音 |
| `rust 1.100 check` | 0 | — | 空 |
| `rust toolchain yaml` | 1 | rusty-todo-md-pre-commit 0★ | 无关 |
| `yaml float version lint` | 1 | 一个 GitHub docs 镜像 | 无关 |
| `unquoted version yaml` | 0 | — | 空 |
| `msrv check ci` | 0 | — | 空 |
| `rust version string compare` | 1 | 无关 | 空 |
| `rust ci lint workflow` | 6 | Holzhaus/ghastly 1★（workflow 安全 linter） | 不查版本 |
| `cargo cooldown` | 4 | dertin/cargo-cooldown 49★（cargo 包装器，自己实现冷却）；NomicFoundation/cargo-cooldown-check 3★（10-21 还在更新，校验 Cargo.lock 里每个依赖都超过 N 分钟） | **相邻**：它们做“强制冷却”，本工具做“开启原生冷却前的预演 + 1.100 体检”，且原生冷却期上线后包装器价值下降 |
| `min-publish-age` | 6 | 全是数据集噪音 | 空 |
| `cargo lockfile age` | 4 | mkbhardwas12/pwned-deps 164★（多语言 lockfile 已知投毒扫描，带 `--min-age`）；bokuweb/sakimori 77★ | 相邻，都不管 CI 版本号 |

相关但不重叠的工具：

- rhysd/actionlint：code search `float` 只命中表达式解析相关文件；issue 搜 `3.10 float` 0 条，未发现对版本号浮点的检查。
- zizmorcore/zizmor：issue 搜 `float version` 没有相关规则（只有 `timeout-minutes: 0.5` 反序列化 bug）。
- yamllint `quoted-strings` 规则可以要求所有字符串加引号，但不懂“这是一个版本号”，在 workflow 里开会产生大量噪音。
- foresterre/cargo-msrv 1,249★：找 MSRV，不看 CI YAML。

结论：没有同类的“Rust 1.100 体检”工具。最近的冷却期工具是包装器/校验器，不做开启前的影响预演。

## 4. 真实样本（GitHub code search）

| 查询 | total | 抽样复核 |
|---|---|---|
| `"toolchain: 1.9" path:.github/workflows` | 9,472 | 分词宽松，需逐个看 |
| `"rust: [1." path:.github/workflows` | 380 | serde、anyhow、num、miette 等都用三段版本（`1.56.0`），**安全** |
| `"msrv: 1." path:.github/workflows` | 278 | tower、ndarray、criterion、RustCrypto 抽样都是三段版本，安全 |
| `"rust-version: 1." path:.github/workflows` | 212 | 待逐个看 |
| `"rust_version: 1." path:.github/workflows` | 852 | 待逐个看 |
| `"rustc --version" "cut -d" path:.github` | 898 | Smithay/wayland-rs、OffchainLabs/nitro 只是取版本做 cache key，安全 |
| `"rustc --version" "sort -V"` | 856 | BowTiedDevil/degenbot、cordx56/rustowl 用 `sort -V` 或注释里明确说避开了字符串比较，安全 |

来自选题调研（`explore-2026-10/rounds/20261022-0813-upcoming`）的未加引号样本：aws/aws-nitro-enclaves-nsm-api、kdl-org/kdl-rs、SolarLiner/interflow、nanoqsh/tauri-wasm、programatik29/axum-server。实现阶段把这些仓库 clone 下来实跑，结果见 `examples/`。

抽样的直接观察：**大多数知名仓库用三段版本号或加了引号**，所以命中率不会高。工具的价值要靠三类检查合起来（YAML + 脚本 + beta diff/冷却期），而不是只靠 YAML 一条。

## 5. 对设计的影响

1. YAML 规则只对“纯标量（plain style）且形如 `数字.数字`”的值报；三段版本、带引号的、`uses:` ref 一律不报。
2. dtolnay/rust-toolchain 的 `toolchain` 输入是已缓解场景，降级为 note，不能报 error（否则对最常见的写法是误报）。
3. 末尾为 0 的两段版本（`1.90`）是**今天就存在的错误**，单独作为 error 报出，这比“将来会坏”更有说服力。
4. 冷却期预演要按 cargo 的真实语义：锁文件里已有的版本不会被挡，报告要区分“会被挡在 `cargo update` 之外”和“会导致解析失败”。
5. 1.100 编译器/Cargo lint 变化用 stable/beta diff 覆盖，不做静态猜测。
