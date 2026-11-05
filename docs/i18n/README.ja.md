<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../assets/banner-dark.svg">
  <img alt="diskgc — 開発者のマシン向けのバックグラウンド・ディスククリーナー" src="../assets/banner-light.svg" width="100%">
</picture>

**削除するものを理解している, 開発者のマシン向けのバックグラウンド・ディスククリーナーです。**

[![CI](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml/badge.svg)](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/eukalpia/diskgc?display_name=tag&sort=semver)](https://github.com/eukalpia/diskgc/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../../LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-555)
![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)

[English](../../README.md) · [Русский](README.ru.md) · [O‘zbekcha](README.uz.md) · [简体中文](README.zh-CN.md) · [Español](README.es.md) · [हिन्दी](README.hi.md) · [العربية](README.ar.md) · [Français](README.fr.md) · [Português](README.pt-BR.md) · **日本語**

<img alt="diskgc のターミナル・ダッシュボード: 概要、すべての操作に理由が付いたプラン、レポートと履歴" src="../assets/dashboard.gif" width="100%">

</div>

## なぜ必要か

開発者のディスクには、誰も残すように指示していないものが溜まっていきます。たとえば、2 日で 100 GB 増える Cargo の `target/` ディレクトリ、7 月に放置したブランチの `node_modules`、テストを実行するたびに再生成される Go のビルドキャッシュ、テストが途中で終了して残ったコンテナ、AI コーディングエージェントの一時ディレクトリなどです。macOS はこれらをまとめて "System Data" と表示するだけで、対処の手助けはしてくれません。`du` は *どこ* に容量を使っているかは教えてくれますが、*何を安全に削除できるか* までは教えてくれません。

diskgc は小さなデーモンとして動作し、空き容量とその推移を監視します。削除するのは、ツールが再生成できるものだけです。すべての操作には理由が記録され、何かが実行される前に読めるプランが用意され、解放した容量の履歴も残ります。

## 主な特徴

- **動作の理由を説明します。** すべての操作には理由が付きます。例: *"target は 105 GB（上限 40 GB 超過）: 1 日より古いものすべて"*、*"プロジェクトは 30 日間更新されていません。install で復元済み"*。
- **フォルダではなくプロジェクトを理解します。** `target/` は中身によって見つけられ、ビルド用ディレクトリはそのプロジェクトのマーカーファイルの隣にある場合だけ対象になります。また、アクティビティはプロジェクト全体で判断されます（git worktree は独立したプロジェクトとして扱われます）。
- **ディスクがいっぱいになる前に動きます。** 直近 6 時間の予測に基づいて、クリーニングを前倒しで実行します。しきい値は *normal* から *low*、*critical* へと段階的に厳しくなります。
- **重要なものには触れません。** ソースコード、`.git`、データベース、チャット履歴は削除候補になりません。除外指定した glob に一致するファイルには触れず、ログは削除ではなくその場で圧縮されます。
- **原因を特定します。** `diskgc stats` は、日ごと・カテゴリごとの増加量と、繰り返し発生するパスを表示します。つまり、掃除ではなく修正すべき場所がわかります。
- **ターミナル・ダッシュボードと JSON 出力。** 6 画面の TUI を備え、すべてのコマンドで `--json` を指定できるためスクリプトからも利用できます。

## クイックスタート

```sh
cargo install --locked --git https://github.com/eukalpia/diskgc diskgc
diskgc config --edit      # [scan] roots に、プロジェクトを置いている場所を設定します
diskgc plan               # clean で何が削除されるかと、その理由を表示します。何も変更されません
diskgc install            # ログイン時にデーモンを起動します（launchd / systemd --user / Task Scheduler）
diskgc status
```

macOS（Apple Silicon、Intel）、Linux（x86_64）、Windows（x86_64）向けのビルド済みバイナリは、各[リリース](https://github.com/eukalpia/diskgc/releases)に添付されています。`diskgc`（必要に応じて `diskgc-ui` も）を展開して、`PATH` の通ったどこかに配置してください。

<div align="center">
<img alt="ターミナルでの diskgc plan: サイズと理由が付いた操作がグループ化されて表示されている" src="../assets/plan.gif" width="100%">
</div>

## 掃除の対象

| ルール | 対象 | タイミング |
|--------|------|-----------|
| Rust の target | `target/` ディレクトリ（中身で検出し、場所は問いません） | 7 日間更新されていない成果物。20 GB を超えると、インクリメンタルキャッシュと大きなテスト用バイナリも対象。40 GB を超えると、1 日より古いものすべて。全 target 合計で 40 GB の上限 |
| node_modules | `package.json` の隣にある `node_modules` | プロジェクト内で 30 日間何も変更されていない |
| ビルド用ディレクトリ | `.dart_tool`、`build`、`_build`、`.next`、`.turbo`、`__pycache__`、`.gradle` など。プロジェクトのマーカーファイルの隣にあるもの | 同じ 30 日ルール |
| 一時ディレクトリ | Claude Code のセッションごとの作業領域、システムの一時ディレクトリ内の自分のエントリ、macOS のユーザー単位 `$TMPDIR` 内の古い作業ディレクトリ | 3 日 / 7 日 / 24 時間、無活動の場合 |
| AI ツール | 古い Codex アプリのバージョン、放置されたランタイムのインストール | 常時（現行版 + 1 つを保持） |
| Docker | 残存した testcontainers、停止中のコンテナ、ぶら下がり状態の volume と image、ビルドキャッシュ | 2 時間 / 24 時間 / 7 日 |
| ツールのキャッシュ | 上限を超えた Go、npm、uv、pnpm、sccache のキャッシュ、Chrome と VS Code の更新の残骸、Homebrew のダウンロード、Xcode の DerivedData、シミュレーターと Metal のシェーダーキャッシュ | ルールごとの上限 |
| 古いファイル | Codex と Claude Code のセッションログと JSONL 履歴 | ファイルシステム（APFS、NTFS）によって**圧縮**され、削除はされません |

## 判断の仕組み

デーモンは 10 分ごとに起動し、空き容量を確認します。

| 空き容量 | 動作 |
|---------|------|
| `comfortable_gb`（150）より多い | 6 時間ごとに定期的な掃除 |
| `comfortable_gb` より少ない | 通常の時間枠ですぐに掃除 |
| `low_gb`（100）より少ない | *low* レベル: すべての経過時間の判定しきい値を半分にします |
| `critical_gb`（60）より少ない | *critical* レベル: 判定しきい値を 4 分の 1 に、サイズ上限を半分にします |

直近 6 時間分のスナップショットに対する最小二乗法によるフィットで、24 時間後の空き容量を予測します。その予測がしきい値を超える場合、デーモンは前倒しでレベルを引き上げます。すべての実行は記録されるため、`diskgc stats` で日ごと・カテゴリごとの増加量と、繰り返し発生する問題の発生源を確認できます。

## 安全性

diskgc はファイルを削除することが仕事であるため、動作が予測可能になるように作られています。

- ツールが再生成できるもの（ビルド成果物、依存関係のディレクトリ、キャッシュ、一時ファイル）のみを提案し、各操作の理由を示します。`plan` と `clean -n` は、何も変更せずにまさにその一覧を表示します。
- ソースコード、`.git`、データベース、チャット履歴は削除候補になりません。`[scan] exclude` に一致するものには触れません。executor は `[safety] allowed_prefixes` の外にあるパスを拒否します（デフォルトでは、ホームディレクトリとシステムの一時フォルダおよびキャッシュフォルダが対象です）。
- `[safety] delete_mode = "trash"` を指定すると、完全に削除する代わりにゴミ箱へ移動します（macOS と Linux。ゴミ箱が別のボリュームにある場合は削除にフォールバックします）。
- すべての実行は `diskgc history` で確認でき、何が計画され、実際に何が解放されたかが記録されます。

それでも、ユーザーの権限のまま無人で動作するため、デーモンをインストールする前に、ご自身のマシンで最初の `plan` の結果をよく確認してください。

## ターミナル・ダッシュボード

`diskgc ui` は、[Cinder](https://github.com/eukalpia/cinder) で作られたダッシュボードを開きます。このダッシュボードは、[IPC プロトコル](../PROTOCOL.md)を通じてデーモンと通信します。

| 画面 | |
|------|---|
| **Overview** | 空き容量、レベル、予測、容量の使われ方 |
| **Plan** | 提案されたすべての操作と理由。残すべきものはチェックを外し、残りを実行するか、ドライランを行えます |
| **Report** | カテゴリごとの最大の問題の発生源 |
| **History** | 実行履歴、日ごとの増加量、繰り返し発生する問題の発生源 |
| **Memory** | プログラムごとの RAM、残存した補助プロセス |
| **Log** | デーモンのライブアクティビティ |

```
1-6 / tab   switch screens        r   refresh the current screen
p           build a plan          space / enter   toggle the highlighted action
x           execute selected      n   dry run             a   toggle all
k           terminate orphaned helper processes (Memory)   q   quit
```

ダッシュボードは独立したネイティブ実行ファイル（`diskgc-ui`）です。`diskgc ui` は、それを `diskgc` の隣または `PATH` 上で探し、デーモンが動いていなければ起動します。

## コマンド

```
diskgc report            ディスクを占有しているもの: カテゴリと最大の問題の発生源
diskgc plan              clean の動作と理由を表示します。何も変更されません
diskgc clean [-n]        今すぐ掃除します（-n = プランの表示のみ）
diskgc status            デーモンの状態、空き容量、予測、最後の実行
diskgc stats             日ごとの増加量、カテゴリごとの傾向、繰り返し発生する問題の発生源
diskgc history           最近の実行と、削除した内容
diskgc mem               プロセスごとの RAM、残存した補助プロセス（--kill-orphans）
diskgc install | uninstall
diskgc config [--edit|--show]
diskgc ui                ターミナル・ダッシュボード
```

すべてのコマンドは `--json` を受け付けます。

## 設定

設定ファイルは `~/.config/diskgc/config.toml`（Linux、macOS）または `%APPDATA%\diskgc\config.toml`（Windows）です。初回実行時にデフォルト値で作成され、デーモンは毎回の周期で再読み込みします。各キーの説明は [docs/CONFIG.md](../CONFIG.md) にあります。主な設定は次のとおりです。

```toml
[scan]
roots = ["~/work", "~/src"]          # プロジェクトを置いている場所
exclude = ["~/work/keep-forever/**"] # どのルールも触れてはならない glob

[thresholds]
comfortable_gb = 150
low_gb = 100
critical_gb = 60

[safety]
delete_mode = "permanent"            # または "trash"
```

## 対応プラットフォーム

| プラットフォーム | 状態 |
|-----------------|------|
| macOS（APFS） | 日常的に使用しており、主な対象です |
| Linux | CI とコンテナでテスト済み。`systemd --user` による自動起動 |
| Windows | CI でビルドとテストに成功。Task Scheduler による自動起動は、最も検証が少ない部分です。フィードバックを歓迎します |

透過的な圧縮は APFS と NTFS で利用できます。プラットフォームごとの対応状況は [docs/ARCHITECTURE.md](../ARCHITECTURE.md) を参照してください。

## ソースからのビルド

```sh
cargo build --release
cargo test
cd ui && dart pub get && dart compile exe bin/diskgc_ui.dart -o build/diskgc-ui
```

この README の録画は、合成したホームディレクトリから再現できます。詳しくは [docs/demo](../demo) を参照してください。

## コントリビュート

Issue と pull request を歓迎します。詳しくは [CONTRIBUTING.md](../../CONTRIBUTING.md) を参照してください。新しいルールには、合成したディレクトリツリーに対するテストが必要です。そのテストでは、削除されるものと、残しておくべきものの両方を示してください。

## ライセンス

[Apache-2.0](../../LICENSE)。

*これは翻訳です。原文は英語版の [README](../../README.md) を参照してください。*
