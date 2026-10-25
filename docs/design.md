# rust1100-check 设计

## 目标与边界

`cargo rust1100-check [PATH]`：只读扫描一个仓库，报告它的 CI、脚本和依赖在 Rust 1.100 上会在哪里出问题。

- 默认只做离线静态检查（R1–R4），几秒内跑完，适合放进 CI。
- 两个需要外部资源的检查按需开启：`--min-publish-age`（访问 crates.io sparse index，R5）、`--beta-diff`（需要本机装了 stable 与 beta，R6）。
- 默认不改用户文件，每条发现都带具体的修复写法。`--fix` 只给 R1/R2 命中的那个 YAML 标量加引号，其余字节不动，修完重新扫描再报告。
- 单条忽略：发现所在行或上一行含 `rust1100-check: ignore` 即丢弃（适用于 R0–R4）。
- 非目标（v0.1 不做）：脚本类规则的自动修复、GitLab/CircleCI 专用的 consumer 分析、完整的依赖重解析。

## 扫描范围

用 `ignore` crate 遍历（遵守 `.gitignore`，跳过 `target/`、`.git/`）。

| 类别 | 文件 |
|---|---|
| CI YAML | `.github/**/*.y{a,}ml`（含 workflows 与本地 composite action）、任意位置的 `action.y{a,}ml`、`.gitlab-ci.yml`、`.circleci/config.yml`、`.travis.yml`、`azure-pipelines.yml`、`.cirrus.yml`、`bitbucket-pipelines.yml`、`docker-compose*.y{a,}ml`、`compose.y{a,}ml` |
| 脚本 | `*.sh`、`*.bash`、`Makefile`、`*.mk`、`justfile`、`*.py`、`*.js`/`*.mjs`/`*.ts`、`build.rs`，以及 CI YAML 里 `run:`/`script:` 的块内容（按原文件行号回报） |
| 依赖 | 根目录 `Cargo.lock`，以及 `cargo metadata --no-deps` 给出的工作区成员依赖要求 |

## 规则

| ID | 默认级别 | 名称 |
|---|---|---|
| R0 | warning | `unparsable-yaml`（CI YAML 解析失败，明确报出而不是静默跳过） |
| R1 | error（Rust key）/ warning（其他 version key）/ note（已被 dtolnay 缓解） | `yaml-float-version-truncated` |
| R2 | warning | `yaml-float-rust-version` |
| R3 | warning | `version-compare-as-string` |
| R4 | warning | `two-digit-minor-pattern` |
| R5 | error / note | `cooldown-unsatisfiable` / `cooldown-young-locked` |
| R6 | 跟随诊断 | `beta-new-diagnostic` |
| R7 | error | `i686-windows-host-toolchain`（2026-10-25 新增） |

### R1 `yaml-float-version-truncated`（error）

- **问题**：未加引号的 `1.90`、`1.100`、`3.10` 会被 YAML 读成 1.9、1.1、3.1。末尾为 0 的值**今天就已经错了**。
- **检测**：用 yaml-rust2 的事件解析器建带行列号的树。只看 plain style 标量；文本匹配 `^\d+\.\d*0$`；所在 key 名（小写）包含 `version`、`rust`、`toolchain`、`msrv` 之一。matrix 里的数组元素取其所属 key。
- **误报控制**：
  - 加了引号的、三段版本（`1.90.0` 本来就是字符串）、`uses:` ref 不报。
  - 只看 CI/compose 文件，不扫任意数据 YAML。
  - **dtolnay/rust-toolchain 缓解**：值直接是 `uses: dtolnay/rust-toolchain@*` 步骤的 `with.toolchain`；或者它是 matrix 值 / workflow、job 级 `env:` 值，且所有 `matrix.<key>` / `env.<key>` 引用都只出现在这类 `toolchain` 输入、`name`、或不和版本字面量比较的 `if`/`continue-on-error` 里，降级为 note（该 action 会把 `1.1` 还原成 `1.100`）。只要有一处引用落在 `run:`（包括 `$KEY`）、cache key、其他 action 输入上，就不算缓解。完全没有引用的 env 值不算缓解（job 里的任何进程都能读到它）。
  - 级别：key 与 Rust 相关（含 `rust`/`toolchain`/`msrv`）为 error；只含 `version`（如 `python-version: 3.10`、`node-version: 20.10`）为 warning，因为是否出错取决于消费方。
- **修复**：加引号，`toolchain: "1.90"`；或写三段 `1.90.0`。

### R2 `yaml-float-rust-version`（warning）

- **问题**：`rust: [1.92, stable]` 今天没问题，但 bump 到 `1.100` 时会变成 1.1。
- **检测**：同 R1 的树遍历；文本匹配 `^1\.\d+$`（末尾非 0，否则归 R1）；key 名包含 `rust`、`toolchain`、`msrv`（不含泛化的 `version`，避免把 `python-version: 3.12` 也报成 Rust 问题）。
- **误报控制**：同 R1 的 dtolnay 分析；完全被 dtolnay 缓解的**不报**（bump 后也不会坏）。
- **修复**：现在就加引号。

### R3 `version-compare-as-string`（warning）

- **问题**：`"1.100" < "1.99"`；`bc`/`awk`/`float()` 下 `1.100 < 1.70`；`sort` 不带 `-V` 时 1.100 排在 1.99 前；`cut -c`/`${v:0:4}` 截出 `1.10`。
- **检测**：逐行。行必须有 **Rust 版本上下文**：出现 `rustc`、`rust_version`/`rust-version`/`rustversion`、`msrv`、`toolchain`（不区分大小写），或使用了本文件里**行首赋值**、右边直接调用 `rustc -V`/`--version`/`-vV` 的变量（`VAR=$(rustc -V)`、`VAR := $(shell rustc --version)`、`v = run(["rustc", "-V"])`）。满足上下文后匹配以下任一（每行最多报一次）：
  - `[[ … < … ]]` / `[[ … > … ]]` / `[ … \< … ]` 字符串比较；
  - 和带引号的版本字面量比较：`>= "1.70"`、`'1.70' <`；
  - 管道进 `bc`、awk 程序文本里的 `<`/`>`、`float(`、`parseFloat(`；
  - `sort` 不带 `-V`/`-t`（只带 `-u` 视为去重，不报）；
  - 有上界的定宽截取：`cut -c1-4`、`${VAR:0:N}`、`[:N]`、`[..N]`、`.substr(`；`cut -c 7-` 这类开放区间不报。
- **误报控制**：没有 Rust 版本上下文的行一律不报；`sort -V`、`sort -t. -k2,2n`、`version_check`/`rustc_version` crate 都视为正确写法。
- **修复**：按数字比较 minor：`[ "$(echo "$v" | cut -d. -f2)" -ge 100 ]`、`sort -V`，或在 build.rs 里用 `rustc_version`/`version_check`/`autocfg`。

### R4 `two-digit-minor-pattern`（warning）

- **问题**：`1\.[0-9]{2}`、`1\.\d\d`、`1\.[0-9][0-9]`（后面不接 `+`/`*`/`{`）匹配不到 `1.100`，或把它截成 `1.10`。
- **检测**：脚本和 YAML `run:` 内容中的上述正则字面量。前缀必须是字面量 `1\.`，这本身就足够特定，不要求额外上下文。
- **修复**：`1\.[0-9]+`。

### R5 冷却期预演（`--min-publish-age "7 days"`）

- **语义依据**（cargo#17335 与 cargo 文档）：deny 模式下，`pubtime` 晚于 `now - age` 的版本会被忽略，**但已在 Cargo.lock 里的版本保留**。所以开启冷却期不会让现有 lockfile 立刻失败，影响在于下一次 `cargo update`、新增依赖、或没提交 lockfile 的库每次 CI 的重新解析。
- **检测**：
  1. 读 Cargo.lock，只看 crates.io 来源（`registry+https://github.com/rust-lang/crates.io-index`、`sparse+https://index.crates.io/`）；git/path/其他 registry 计数后跳过。
  2. 并发拉取每个 crate 的 sparse index 文件，读每个版本的 `pubtime`、`yanked`、`deps`。
  3. 锁定版本比 age 新的，算“回退版本”：满足图上所有对它的版本要求（来自父包 index 条目里的 `req`，工作区成员的要求来自 `cargo metadata --no-deps`）、未 yank、且足够老的最新版本。父包自己也年轻时，用父包回退版本的要求重新计算，迭代到不再变化（真实仓库里 `ureq 3.5.0 → ureq-proto 0.6.5` 这种链条需要这一步，否则会把子包误报成无法解析）。
  4. 有回退版本 → `cooldown-young-locked`（note），说明重新解析会选哪个版本；没有 → `cooldown-unsatisfiable`（error），重新解析会失败，需要 `CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow` 或等待。
- **误报控制 / 限制**：按单包近似，不做完整重解析（回退版本自身的依赖可能不同）；拿不到 `pubtime` 的版本明确报错而不是当作“足够老”；没有 Cargo.lock 时直接报错并提示先 `cargo generate-lockfile`。
- **修复**：输出可直接粘贴的 `.cargo/config.toml` 片段。

### R6 beta 新诊断（`--beta-diff`）

- **检测**：用 `rustup run <toolchain>` 分别以基线（`--baseline`，默认 `stable`）和 `beta` 运行 `cargo check --workspace --all-targets --message-format=json`，各用独立的 `--target-dir`（`target/rust1100-check/<toolchain>`）。收集 `compiler-message` 的 (level, code, 主 span 文件, 行, 消息)，以及 cargo 自身在 stderr 打印的 `warning:`/`error:`（新的 Cargo lints 走这里）。只报 beta 有、stable 没有的。
- **误报控制**：键里带行号和消息，同一位置同一诊断不会重复；工具链缺失时明确报错并给出 `rustup toolchain install beta`。
- 1.100 的已知相关变化（never type fallback、`aligned_fields_in_packed`、重复 `repr`、Cargo lints 等）在 `docs/research.md` 列出，报告里附上 release notes 链接。

### R7 `i686-windows-host-toolchain`（error，2026-10-25 新增）

- **问题**：官方博客（2026-10-02，“Demoting i686 Windows targets to std-only”）：从 1.100.0 起 `i686-pc-windows-msvc` 降为 Tier 1 without host tools，`i686-pc-windows-gnu` 降为 Tier 2 without host tools；std 继续发布，编译器等主机工具不再提供，需要从受支持的主机（如 64 位 Windows）交叉编译。实查 dist manifest：stable 1.99.1 仍有 i686 Windows 的 rustc/cargo；beta 1.100.0-beta.7 和当前 nightly 已经没有，只剩 rust-std。按日期二分：`-gnu` 的主机工具从 nightly-2026-08-27 起消失，`-msvc` 从 nightly-2026-09-10 起消失。
- **检测**：
  - 字面量：rustup 工具链名 = 渠道或版本前缀 + 三元组，`(stable|beta|nightly(-YYYY-MM-DD)?|1.N(.P)?)-i686-pc-windows-(msvc|gnu)`；在 CI YAML（含 `run:` 块、matrix 值、action 输入）、脚本、`rust-toolchain`/`rust-toolchain.toml` 的非注释行里逐行匹配。另外匹配 `default-host i686-pc-windows-*`（`rustup set default-host`、`rustup-init --default-host`）。
  - matrix 展开：GitHub job 里含 `${{ matrix.x }}` 的字符串，用 matrix 各轴和 `include` 的取值代入后再匹配（上限 256 种组合），覆盖 `toolchain: ${{ matrix.version }}-${{ matrix.toolchain }}` 这类拼出来的名字。展开结果里的名字如果已在文件中字面出现，只在字面位置报一次；`name:` 不参与展开。
  - 是否受影响：`stable` 报（11-12 起坏）；`beta`/`nightly` 报（现在就坏）；`nightly-日期` 按上面的分界日期判断；固定版本 ≥ 1.100 报，< 1.100 不报。
- **误报控制**：裸三元组 `i686-pc-windows-msvc` 一律视为 target，所以 `target:`/`targets:`、`rustup target add`、`cargo build --target`、`target/i686-…/release` 路径都不报；注释行不报。`exclude` 不参与展开，可能把被排除的组合也算进去（过近似，未在样本中遇到）。
- **修复**：x86_64 主机工具链 + `rustup target add i686-pc-windows-*`（或 toolchain action 的 `targets:`）+ `cargo build --target i686-pc-windows-*`。

## 输出

`--format text|json|github|sarif`，默认 text。

- **text**：按文件分组，`path:line:col  severity  rule  message`，下面一行 `help:`；最后一行汇总。
- **json**：`{"tool":…, "version":…, "findings":[{rule, severity, file, line, column, message, help}], "summary":{error, warning, note}}`。
- **github**：`::error file=…,line=…,col=…,title=rust1100-check/<rule>::<message>`，在 PR diff 上显示注解。
- **sarif**：SARIF 2.1.0，`runs[0].tool.driver.rules` 列出全部规则，可交给 `github/codeql-action/upload-sarif`。

退出码：0 没有达到 `--fail-on` 级别的发现（默认 error）；1 有；2 运行错误（参数错误、网络错误、缺工具链）。

## GitHub Action 包装（可选）

仓库根目录 `action.yml`，composite action：

1. 按 `RUNNER_OS`/`RUNNER_ARCH` 从 GitHub Release 下载 `cargo-rust1100-check-<target>.tar.gz`（Linux x86_64、macOS arm64/x86_64）；没有匹配平台时 `cargo install --git … --tag v<version>`。不依赖 crates.io。
2. 用 bash 数组拼参数运行 `--format github`，输入 `path`、`min-publish-age`、`fail-on` 分开传，不用 `eval`。

需要 SARIF 时用户自行加 `--format sarif > out.sarif` 再接 `upload-sarif`，README 给示例。

## 代码结构

```
src/main.rs        CLI（clap，cargo 子命令入口）
src/lib.rs         run()、Finding、Severity
src/yaml.rs        带位置的 YAML 树 + R1/R2 + run 块提取
src/script.rs      R3/R4
src/cooldown.rs    R5
src/beta.rs        R6
src/report.rs      四种输出格式
tests/             fixtures + 集成测试
```

依赖：clap、yaml-rust2、regex、ignore、serde/serde_json、toml、semver、ureq、jiff。
