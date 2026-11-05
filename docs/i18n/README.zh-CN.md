<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../assets/banner-dark.svg">
  <img alt="diskgc——面向开发者机器的后台磁盘清理工具" src="../assets/banner-light.svg" width="100%">
</picture>

**一个知道自己在删除什么的开发者机器后台磁盘清理工具。**

[![CI](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml/badge.svg)](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/eukalpia/diskgc?display_name=tag&sort=semver)](https://github.com/eukalpia/diskgc/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../../LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-555)
![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)

[English](../../README.md) · [Русский](README.ru.md) · [O‘zbekcha](README.uz.md) · **简体中文** · [Español](README.es.md) · [हिन्दी](README.hi.md) · [العربية](README.ar.md) · [Français](README.fr.md) · [Português](README.pt-BR.md) · [日本語](README.ja.md)

<img alt="diskgc 终端仪表盘：概览、为每项操作附带原因的计划、报告与历史" src="../assets/dashboard.gif" width="100%">

</div>

## 为什么需要它

开发者的磁盘总会被一些没人想保留的东西占满：两天内增长 100 GB 的 Cargo `target/` 目录、七月就已放弃的分支里的 `node_modules`、每次测试都会重新填满的 Go 构建缓存、被中断的测试遗留下来的容器，以及 AI 编程助手的临时目录。macOS 把这些统称为"系统数据"（System Data），却不提供任何帮助；`du` 只能告诉你*在哪里*，无法告诉你*什么可以安全删除*。

diskgc 以一个小型守护进程的形式运行，监视可用空间及其变化趋势，只删除工具能够重新生成的内容。每项操作都附有明确的原因，任何操作发生前你都可以先阅读计划，清理结束后还能查看已释放空间的历史记录。

## 亮点

- **自我解释。** 每项操作都附带原因，例如 *"target 占用 105 GB（超过 40 GB 上限）：删除一天以前的所有内容"*、*"项目 30 天未改动；可通过 install 恢复"*。
- **理解项目，而不只是文件夹。** `target/` 通过其内容识别；构建目录只有在项目标记文件旁边才会被识别；活跃度以整个项目为单位判断（git worktree 视为独立项目）。
- **在磁盘写满之前行动。** 基于过去六小时快照的预测会提前加大清理力度；清理阈值会从*正常*逐级收紧到*低*和*严重*。
- **从不触碰重要内容。** 源代码、`.git`、数据库和聊天记录永远不会成为候选对象；被排除的 glob 永远不会被触碰；日志是原地压缩，而不是直接删除。
- **找到根本原因。** `diskgc stats` 按天、按类别展示增长情况，并列出反复出现的路径——这些才是应该修复而不是反复清理的地方。
- **终端仪表盘与 JSON 输出。** 提供一个六屏的 TUI，并且每个命令都支持 `--json`，方便脚本调用。

## 快速开始

```sh
cargo install --locked --git https://github.com/eukalpia/diskgc diskgc
diskgc config --edit      # 将 [scan] roots 设置为你的项目所在目录
diskgc plan               # 查看清理会删除什么及原因，此时不会改动任何文件
diskgc install            # 登录时启动守护进程（launchd / systemd --user / 任务计划程序）
diskgc status
```

每个[发布版本](https://github.com/eukalpia/diskgc/releases)都附带了预编译的二进制文件，支持 macOS（Apple Silicon、Intel）、Linux（x86_64）和 Windows（x86_64）。解压 `diskgc`（可选再解压 `diskgc-ui`）到你的 `PATH` 中的任意目录即可。

<div align="center">
<img alt="终端中的 diskgc 计划：按组列出的操作，附带大小和原因" src="../assets/plan.gif" width="100%">
</div>

## 清理范围

| 规则 | 内容 | 触发时机 |
|------|------|------|
| Rust 构建产物 | `target/` 目录（按内容识别，无论位于何处） | 产物 7 天未被修改；超过 20 GB 时还会清理增量缓存和体积较大的测试二进制文件；超过 40 GB 时清理一天以前的所有内容；所有 target 合计预算为 40 GB |
| node_modules | 位于 `package.json` 旁边的 `node_modules` | 项目内容 30 天未改动 |
| 构建目录 | `.dart_tool`、`build`、`_build`、`.next`、`.turbo`、`__pycache__`、`.gradle` 等，位于项目标记文件旁边 | 同样的 30 天规则 |
| 临时目录 | Claude Code 每个会话的临时目录、系统临时目录中你自己的条目、macOS 上每用户 `$TMPDIR` 中过期的工作目录 | 3 天 / 7 天 / 24 小时无变动 |
| AI 工具 | 旧版 Codex 应用版本、已废弃的运行时安装 | 始终清理（保留当前版本及其前一个版本） |
| Docker | 孤立的 testcontainers 容器、已停止的容器、悬空的卷和镜像、构建缓存 | 2 小时 / 24 小时 / 7 天 |
| 工具缓存 | 超过上限的 Go、npm、uv、pnpm 和 sccache 缓存，Chrome 与 VS Code 的更新残留，Homebrew 下载内容，Xcode DerivedData，模拟器和 Metal 着色器缓存 | 按规则设定的上限 |
| 冷文件 | Codex 和 Claude Code 的会话日志与 JSONL 历史 | 由文件系统**压缩**（APFS、NTFS），从不删除 |

## 决策方式

守护进程每 10 分钟唤醒一次，检查可用空间：

| 可用空间 | 行为 |
|------------|------|
| 高于 `comfortable_gb`（150） | 每 6 小时进行一次定期清理 |
| 低于 `comfortable_gb` | 按正常的时间窗口立即清理 |
| 低于 `low_gb`（100） | *低*级别：所有闲置时间窗口减半 |
| 低于 `critical_gb`（60） | *严重*级别：时间窗口缩短为原来的四分之一，大小上限减半 |

基于过去六小时快照的最小二乘拟合会预测 24 小时后的可用空间；如果预测结果将越过某个阈值，守护进程会提前升级清理级别。每次运行都会被记录，因此 `diskgc stats` 可以展示按天、按类别的增长情况，以及反复出现的问题项。

## 安全性

diskgc 会删除文件，这正是它的职责，因此它的设计目标是行为可预测：

- 它只提议工具能够重新生成的内容（构建产物、依赖目录、缓存、临时文件），并为每项操作给出原因；`plan` 和 `clean -n` 会精确列出这些内容，而不会改动任何文件。
- 源代码、`.git`、数据库和聊天记录永远不会成为候选对象；匹配 `[scan] exclude` 的内容永远不会被触碰；执行器会拒绝 `[safety] allowed_prefixes` 之外的任何路径（默认为你的主目录以及系统的临时和缓存目录）。
- `[safety] delete_mode = "trash"` 会将内容移入回收站而不是永久删除（适用于 macOS 和 Linux；若回收站位于另一个卷上，则回退为直接删除）。
- 每次运行都会记录日志（`diskgc history`），包括计划了什么以及实际释放了多少空间。

它仍会以你的权限无人值守地运行，因此在安装守护进程之前，请先阅读你机器上的第一份 `plan`。

## 终端仪表盘

`diskgc ui` 会打开一个基于 [Cinder](https://github.com/eukalpia/cinder) 构建的仪表盘，它通过其 [IPC 协议](../PROTOCOL.md) 与守护进程通信：

| 界面 | |
|--------|---|
| **概览** | 可用空间、级别、预测，以及空间都去了哪里 |
| **计划** | 每项拟执行的操作及其原因——取消勾选应保留的项，执行其余项，或进行试运行 |
| **报告** | 每个类别中占用最大的项目 |
| **历史** | 运行记录、按天的增长情况、反复出现的问题项 |
| **内存** | 按程序统计的内存占用、孤立的辅助进程 |
| **日志** | 守护进程的实时活动 |

```
1-6 / tab   switch screens        r   refresh the current screen
p           build a plan          space / enter   toggle the highlighted action
x           execute selected      n   dry run             a   toggle all
k           terminate orphaned helper processes (Memory)   q   quit
```

仪表盘是一个独立的原生可执行文件（`diskgc-ui`）。`diskgc ui` 会在 `diskgc` 旁边或 `PATH` 中查找它，如果没有正在运行的守护进程，则会将其启动。

## 命令

```
diskgc report            磁盘占用情况：各类别及占用最大的项目
diskgc plan              查看清理将执行的操作及原因，不会改动任何文件
diskgc clean [-n]        立即清理（-n 表示仅生成计划）
diskgc status            守护进程状态、可用空间、预测、上次运行情况
diskgc stats             按天的增长、类别趋势、反复出现的问题项
diskgc history           最近的运行记录及其删除的内容
diskgc mem               按进程统计的内存占用、孤立的辅助进程（--kill-orphans）
diskgc install | uninstall
diskgc config [--edit|--show]
diskgc ui                终端仪表盘
```

每个命令都接受 `--json` 参数。

## 配置

`~/.config/diskgc/config.toml`（Linux、macOS）或 `%APPDATA%\diskgc\config.toml`（Windows）。首次运行时会以默认值创建，守护进程每个周期都会重新读取。每个配置项的说明见 [docs/CONFIG.md](../CONFIG.md)。以下是最关键的部分：

```toml
[scan]
roots = ["~/work", "~/src"]          # 你的项目所在目录
exclude = ["~/work/keep-forever/**"] # 任何规则都不得触碰的 glob

[thresholds]
comfortable_gb = 150
low_gb = 100
critical_gb = 60

[safety]
delete_mode = "permanent"            # 或 "trash"
```

## 平台

| 平台 | 状态 |
|----------|--------|
| macOS（APFS） | 每日使用，是主要目标平台 |
| Linux | 在 CI 和容器中测试；使用 `systemd --user` 实现开机自启 |
| Windows | 可在 CI 中构建并通过测试；通过任务计划程序实现自启是测试最少的部分，欢迎反馈 |

APFS 和 NTFS 支持透明压缩。平台矩阵详见 [docs/ARCHITECTURE.md](../ARCHITECTURE.md)。

## 从源码构建

```sh
cargo build --release
cargo test
cd ui && dart pub get && dart compile exe bin/diskgc_ui.dart -o build/diskgc-ui
```

本 README 中的录屏可以从一个合成的主目录中复现，参见 [docs/demo](../demo)。

## 参与贡献

欢迎提交 Issue 和 Pull Request，参见 [CONTRIBUTING.md](../../CONTRIBUTING.md)。新增规则需要附带一个测试，该测试针对一个合成的目录树，同时展示它会删除什么以及必须保留什么。

## 许可证

[Apache-2.0](../../LICENSE)。

*本文为译文，内容以英文版 [README.md](../../README.md) 为准。*
