# docs/DEVELOPMENT.md — 開発・xtask・配布ビルド・リポジトリ構成

作業ルールは [AGENTS.md](../AGENTS.md)、仕様は [SPEC.md](../SPEC.md)、全設定キーは [SETTINGS.md](SETTINGS.md)。

## 必要なもの

| 対象 | 内容 |
|---|---|
| Rust | `cargo` 1.88 以上（`rust-version` に合わせる） |
| Tauri CLI | `cargo install tauri-cli --version '^2'` |
| Windows | WebView2 ランタイム（Windows 11 は標準搭載） |
| Linux | 下の apt 一覧 |
| Python | ML工程を動かす場合のみ。PicoVTuber 管理下の専用環境を `cargo xtask setup runtime` が用意する（基準は Python 3.12） |

**Node.js はアプリと開発タスクの依存にしない。** 作業用ツールとしては使ってよいが、`package.json` を置いて依存を増やす形にはしない。

### Linux のシステムライブラリ

`.claude/hooks/session-start.sh` の `APT_PACKAGES` がこの一覧の正本。変更したら README のコマンドも同じ作業で同期する。

```bash
sudo apt-get install -y --no-install-recommends \
  pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev \
  libayatana-appindicator3-dev libasound2-dev libclang-dev libxcb1-dev \
  libxrandr-dev libdbus-1-dev libwayland-dev libegl-dev libgbm-dev
```

## xtask

開発タスクは `cargo xtask <サブコマンド>`（エイリアスは `.cargo/config.toml`）。

| コマンド | 内容 |
|---|---|
| `cargo xtask setup viewer` | three.js / three-vrm を `ui/vendor/` へ取得（git 管理外） |
| `cargo xtask setup runtime` | ML工程用の Python 環境を PicoVTuber 管理下へ用意 |
| `cargo xtask setup models` | 生成モデルの重みを取得。**GPU種別・OSを判定し、使えない構成の重みは取らない** |
| `cargo xtask setup all` | 上の全部（明示操作のときだけ。起動や通常ビルドへ混ぜない） |
| `cargo xtask build-ui` | `ui/` を Tauri 配信用の `ui-dist/` へ差分ビルド |
| `cargo xtask dev` | `build-ui` のあと `tauri dev` |
| `cargo xtask build` | 製品ビルド（`src-tauri/target/release/bundle/`） |
| `cargo xtask verify` | PR前の一括ローカル検証（下記） |
| `cargo xtask stop` | 残存プロセスの回収 |
| `cargo xtask bump-version <major\|minor\|patch>` | 版番号の同期更新（手作業で複数箇所を書き換えない） |

### `cargo xtask verify` が回すもの

途中で失敗しても最後まで実行し、末尾に失敗とスキップをまとめて出す。

1. 本体と xtask の書式（`cargo fmt --check`）
2. 全 target の静的解析（`cargo clippy --all-targets -- -D warnings`）
3. テスト（`cargo test --lib`。設計ガードもここで走る）
4. 共通スキル正本（`.agents/skills/`）と Claude Code 用自動認識ファイル（`.claude/skills/`）の同期
5. 追跡している JavaScript / MJS の構文
6. Git 差分（未追跡の生成物が混ざっていないか）

## 検証コストの順序

`AGENTS.md` の規則どおり、**軽い順に使う**。

1. `cargo check --manifest-path src-tauri/Cargo.toml` — コンパイルが通るか
2. `cargo test --lib --manifest-path src-tauri/Cargo.toml` — ロジックと設計ガード
3. `cargo xtask verify` — PR前の総仕上げ
4. `cargo xtask dev` / 生成パイプラインの実走 / フルビルド — **高コスト。目視が本当に必要なときだけ、確認をまとめて1回**

## Claude Code on the web で作業する場合

`.claude/hooks/session-start.sh` が環境を用意する。clone 直後は次の2点で `cargo check` すら通らないため、手で毎回入れ直さないこと。

1. Tauri / cpal が要求するシステムライブラリが無い
2. `tauri.conf.json` の `frontendDist` が指す `ui-dist/` が Git 管理外で存在せず、`tauri::generate_context!` がコンパイル時に落ちる

フックは冪等・非対話で、`CLAUDE_CODE_REMOTE!=true`（ローカルの Windows / macOS）では即 exit する。

**コンテナには GPU もマイクも配信ソフトも無い。** ML工程の実走、リップシンクの実音声、配信出力の取り込みはローカル実機で行う。Rust の `cargo check` / `cargo clippy` / `cargo test --lib` は素材が無くても通る。

## コネクタを足す

中央のレジストリは編集しない。`build.rs` がディレクトリを走査して `#[path] pub mod` を生成する。

```
生成工程   src-tauri/src/pipeline/stages/<工程名>.rs   Stage を実装 + inventory::submit!
配信出力   src-tauri/src/studio/outputs/<出力名>.rs    Output を実装 + inventory::submit!
```

ファイル名（＝モジュール名）は英小文字・数字・アンダースコアだけ。ハイフンを入れると生成コードが構文エラーになるので、`build.rs` がその場で理由を出して止める。

手順の正本は `.agents/skills/add-picovtuber-connector/SKILL.md`。設定項目を伴う場合は `.agents/skills/add-picovtuber-setting/SKILL.md` も読む。

## リポジトリ構成

フォルダ構成は [AGENTS.md](../AGENTS.md) の「フォルダ構成」が正本。ここでは Git 管理外のものだけ挙げる。

| パス | 生成方法 | 理由 |
|---|---|---|
| `ui-dist/` | `cargo xtask build-ui` | `ui/` からの生成物。正本は `ui/` |
| `ui/vendor/` | `cargo xtask setup viewer` | 第三者ライブラリを再配布しない |
| `src-tauri/resources/bundled/` | `cargo xtask setup` | 同梱物を再配布しない |
| `temp/` | 作業中 | 一時作成物はここだけに置く |

## 版番号

SemVer。`cargo xtask bump-version` が正規入口で、アプリ Cargo・xtask Cargo・Tauri 設定を同期する。
`minor` / `major` はユーザーの明示指示があるときだけ。エージェントが機能内容から独断で上げない。
