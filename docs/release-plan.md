# 发布计划（待确认，尚未执行任何一步）

## 发布步骤

1. 在 GitHub 建 `majiayu000/rust1100-check`（public），推 `main`。
2. 打 tag `v0.1.0`，另建 `v0` 移动 tag 供 Action 用户引用（README 示例写的是 `@v0.1.0`）。
3. `cargo publish --dry-run`，确认无误后 `cargo publish`（包名 `rust1100-check` 在 crates.io 上未被占用需发布前再查一次）。
4. 发布后把 README 安装方式从 `--git` 改成 `cargo install rust1100-check --locked`，`action.yml` 默认 `version` 与 tag 保持一致。
5. 可选：在本仓库加一个 CI workflow（`cargo test` + `cargo clippy -- -D warnings` + 用自己的 Action 扫自己）。注意 `tests/fixtures/scripts` 下的样本会被自扫命中，需要加 `.ignore` 或给 Action 传 `tests/` 之外的路径。

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

## 发布前需要你决定的事

1. **许可证**：Cargo.toml 写的是 `MIT OR Apache-2.0`，仓库里还没有 LICENSE 文件。是否沿用、是否补两个 LICENSE 文件。
2. **对外口径**：真实数据显示知名大仓库（ripgrep、tokio、clap、serde、anyhow、ndarray、zino）都没有 error/warning，命中集中在小仓库和 actions-rs 用户。帖子标题是否保留 “installs Rust 1.1.0” 这种强钩子，还是改成更保守的说法。
3. **是否给命中的仓库提 issue/PR**（vinezombie、interflow、kdl-rs）。计划默认不提。
4. **Action 的 `eval`**：`action.yml` 用 `eval` 拆分 `args`，以支持 `--min-publish-age "7 days"` 这种带空格的参数；输入来自 workflow 作者本人，但 zizmor 一类工具可能报警。可改成拆成几个独立 input。
5. **中文渠道**（V2EX、掘金、Rust 中文社区）是否同步发。
