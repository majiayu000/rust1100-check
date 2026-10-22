# 发布计划

## 发布步骤

1. 建 `majiayu000/rust1100-check`（public），推 `main`，CI（test、clippy、MSRV 1.88、自扫）跑绿。
2. 打 tag `v0.1.0`，`release.yml` 构建三平台二进制并挂到 Release。
3. crates.io（需要你操作）：`cargo publish --dry-run` 后 `cargo publish`。2026-10-22 查询 `https://crates.io/api/v1/crates/rust1100-check` 返回 404，名字未被占用。
4. 发布到 crates.io 后把 README 安装方式改成 `cargo install rust1100-check --locked`。

## 时间线

| 日期 | 动作 |
|---|---|
| 10-23 ~ 11-04 | 每周用最新 beta（`rustup update beta`）重跑 `examples/` 里的 beta diff；根据反馈补规则（例如 GitLab `parallel: matrix`） |
| 11-04（周三） | 投 This Week in Rust（当期截稿时间以 TWiR 仓库为准） |
| **11-05 软发布** | GitHub + crates.io 上线；发 r/rust 和 users.rust-lang.org。标题方向：“Rust 1.100 lands Nov 12. Unquoted `1.100` in your CI YAML installs Rust 1.1.0. A checker.” 正文用 `examples/README.md` 的真实数字，明确写出大仓库 0 命中 |
| 11-05 ~ 11-11 | 收 issue；如有人报误报，优先修 |
| 11-11 | 再投一期 TWiR |
| **11-12 Show HN** | 1.100 正式发布当天发 Show HN；同时在官方发布帖下回复。当天把 README 的 “Until Nov 12, beta is 1.100” 改为基于 stable 的说法，`--beta-diff` 的意义转为“提前看 1.101” |
| 11-22 | 窗口结束，复盘 |

## 已定

- 许可证 MIT（作者 majiayu000），GitHub 仓库公开、Release v0.1.0 附三平台二进制（2026-10-22 用户批准）。
- Action 改为下载 Release 二进制，去掉 `eval`。

## 仍待你决定

1. **对外口径**：真实数据显示知名大仓库（ripgrep、tokio、clap、serde、anyhow、ndarray、zino）都没有 error/warning，命中集中在小仓库和 actions-rs 用户。帖子标题是否保留 “installs Rust 1.1.0” 这种强钩子。
2. **是否给命中的仓库提 issue/PR**（vinezombie、interflow、kdl-rs）。计划默认不提。
3. **crates.io 发布**：需要你的 token，命令见汇报。
4. **中文渠道**（V2EX、掘金、Rust 中文社区）是否同步发。
