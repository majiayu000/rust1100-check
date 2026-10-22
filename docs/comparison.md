# 与相邻工具的功能对比

数据取自 2026-10-22（`gh api repos/...` 的 star 数与最近 push 时间；功能以各项目 README / 文档为准）。

## 相邻工具一览

| 工具 | ★ | 最近 push | 做什么 |
|---|---|---|---|
| zizmorcore/zizmor | 6,754 | 2026-10-22 | GitHub Actions 安全静态分析（41 个 audit：模板注入、未固定 uses、权限过大、dependabot-cooldown 等） |
| rhysd/actionlint | 4,343 | 2026-07-16 | GitHub Actions workflow 语法/类型检查，集成 shellcheck、pyflakes |
| adrienverge/yamllint | 3,479 | 2026-09-19 | 通用 YAML 风格检查（含 `quoted-strings`） |
| EmbarkStudios/cargo-deny | 2,460 | 2026-10-21 | 依赖许可证、漏洞公告、重复版本、来源检查 |
| obi1kenobi/cargo-semver-checks | 1,697 | 2026-10-20 | 检查 crate 公共 API 的 semver 破坏 |
| dtolnay/rust-toolchain | 1,592 | 2026-10-16 | 安装 Rust 工具链的 Action；内置把 `1.1` 还原为 `1.100` 的启发式 |
| foresterre/cargo-msrv | 1,249 | 2026-10-22 | 二分查找/验证 crate 的 MSRV |
| actions-rust-lang/setup-rust-toolchain | 407 | 2026-09-07 | 安装 Rust 工具链 + 缓存的 Action（版本原样传给 rustup） |
| arnica/depsguard | 386 | 2026-08-12 | 加固包管理器配置（npm/pnpm/yarn/bun/uv/pip/poetry）；代码搜索 `min-publish-age` 0 命中，**不支持 cargo 冷却期** |
| mkbhardwas12/pwned-deps | 164 | 2026-10-18 | 扫 lockfile 里的已知投毒版本，带 `--min-age` |
| dertin/cargo-cooldown | 49 | 2026-10-05 | cargo 包装器，自己实现冷却期 |
| NomicFoundation/cargo-cooldown-check | 3 | 2026-10-21 | 校验 Cargo.lock 里每个依赖都超过 N 分钟 |

## 逐项对比

✅ 支持　⚠️ 部分　— 不支持

| 能力 | rust1100-check | actionlint | zizmor | yamllint | dtolnay/rust-toolchain | cargo-msrv | cargo-cooldown(-check) | cargo-deny |
|---|---|---|---|---|---|---|---|---|
| 发现未加引号、会被 YAML 读成浮点的版本号（`1.90`→1.9、`1.100`→1.1） | ✅ | —（只在字段**要求**数字时做类型检查） | — | ⚠️ `quoted-strings` 要求所有字符串加引号，不区分版本号，噪音大 | ⚠️ 只修复自己的 `toolchain` 输入 | — | — | — |
| 区分“已被 dtolnay 缓解”与“会装错版本”（跟踪 `matrix.*`/`env.*` 的使用位置） | ✅ | — | — | — | — | — | — | — |
| GitLab CI / CircleCI / Travis / compose 文件 | ✅ | — | ⚠️ 只看 GitHub Actions 与 Dependabot | ✅（通用） | — | — | — | — |
| 脚本里按字符串/浮点比较 Rust 版本、只匹配两位 minor 的正则 | ✅ | ⚠️ shellcheck 不懂版本语义 | — | — | — | — | — | — |
| 自动加引号修复 | ✅ `--fix` | — | ⚠️ 有 `--fix`，但针对安全 audit | — | — | — | — | — |
| stable 与 beta(1.100) 编译诊断差异（新 lint、新 Cargo lints） | ✅ `--beta-diff` | — | — | — | — | ⚠️ 只判断能不能编译过 | — | — |
| Cargo 原生冷却期（`global-min-publish-age`）开启前预演 | ✅ 按 cargo 语义：已锁定的保留，算回退版本、找出无法解析的要求 | — | ⚠️ `dependabot-cooldown` 只检查 Dependabot 配置 | — | — | — | ⚠️ 自己实现冷却，或只做“是否够老”的通过/失败 | — |
| 生成可直接粘贴的 `.cargo/config.toml` 片段 | ✅ | — | — | — | — | — | — | — |
| 输出格式 | text / JSON / GitHub 注解 / SARIF | text / JSON / SARIF 等 | text / JSON / SARIF / GitHub | text / parsable / GitHub | — | text / JSON | text | text / JSON |
| GitHub Action | ✅（下载预编译二进制） | ⚠️ 第三方包装（reviewdog/action-actionlint） | ✅ | — | 本身就是 Action | —（仓库里没有 action.yml） | — | ✅ EmbarkStudios/cargo-deny-action |
| 忽略单条发现 | ✅ 行内 `rust1100-check: ignore` | ⚠️ `-ignore '<正则>'` 命令行参数 | ✅ 行内 `zizmor: ignore[...]` | ✅ `# yamllint disable-line` | — | — | — | ⚠️ 在 deny.toml 里配置 |

## 结论

- 没有工具覆盖“Rust 1.100 三位数版本号”这一层：actionlint/zizmor 关注语法和安全，yamllint 不懂版本语义，dtolnay 只救自己的输入。
- 冷却期方面，现有工具要么自己实现冷却、要么只做阈值校验；本工具按 cargo 1.100 的真实语义（锁文件里的版本保留）预演，并给出回退版本和失败原因。
- 本工具不做的：安全审计（用 zizmor）、通用 workflow 语法检查（用 actionlint）、MSRV 搜索（用 cargo-msrv）、许可证/漏洞（用 cargo-deny）。建议和它们并用，而不是替代。

## 首次使用会期待、而 v0.1 起初缺的功能（已补）

1. **`--fix`**：发现了一堆未加引号的版本号，用户第一反应是“帮我全部加上引号”。只改 R1/R2 命中的那个标量，其余字节不动。
2. **行内忽略**：放进 CI 后，总有一两处是有意为之（例如 job 名里的数字）。`# rust1100-check: ignore` 写在同一行或上一行即可。
3. **Action 不依赖 crates.io**：原 `action.yml` 用 `cargo install rust1100-check`，而 crate 尚未发布，Action 会直接失败。改为从 GitHub Release 下载预编译二进制，没有匹配平台时回退到 `cargo install --git`。
