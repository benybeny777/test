#!/bin/bash
# session-start.sh - Claude Code on the web のセッション開始時に、cargo が通る状態を作る。
#
# リポジトリを clone しただけでは Linux で `cargo check` / `cargo test` が失敗する。
# 原因は2つあり、どちらも毎回同じ手順で解消できるのでここに固定する。
#   1. Tauri / cpal が要求するシステムライブラリ（gdk-3.0・alsa など）が入っていない。
#   2. `tauri.conf.json` の frontendDist が指す `ui-dist/` が Git 管理外で存在しない。
#      `tauri::generate_context!` はコンパイル時にこのパスの実在を確かめるため、
#      無いとビルドがマクロの段階で止まる。
#
# Ubuntu Linux も対応OSなので、Tauri本体に加えてX11／Waylandの依存も揃える。
set -euo pipefail

# ローカル（Windows / macOS）の開発環境には触らない。web セッションでだけ動かす。
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"

log() { printf '[session-start] %s\n' "$1"; }

# ── 1. システムライブラリ ───────────────────────────────────────────
# Tauri v2 の Linux 前提パッケージ ＋ 本プロジェクト固有の依存:
#   libasound2-dev … cpal（マイク入力によるリップシンク）が使う alsa-sys
# この一覧が正本。変更したら README.md と docs/DEVELOPMENT.md の apt コマンドも
# 同じ作業で同期する（AGENTS.md のドキュメント同期ルール）。
APT_PACKAGES=(
  pkg-config
  libgtk-3-dev
  libwebkit2gtk-4.1-dev
  libsoup-3.0-dev
  librsvg2-dev
  libayatana-appindicator3-dev
  libasound2-dev
  libclang-dev
  libxcb1-dev
  libxrandr-dev
  libdbus-1-dev
  libwayland-dev
  libegl-dev
  libgbm-dev
)

if [ "$(id -u)" -eq 0 ]; then
  SUDO=""
else
  SUDO="sudo"
fi

missing=()
for package in "${APT_PACKAGES[@]}"; do
  if ! dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q "install ok installed"; then
    missing+=("$package")
  fi
done

if [ ${#missing[@]} -eq 0 ]; then
  log "システムライブラリは導入済み（${#APT_PACKAGES[@]} 件）"
else
  log "システムライブラリを導入: ${missing[*]}"
  export DEBIAN_FRONTEND=noninteractive
  # 一部の第三者 PPA はプロキシ経由で 403 を返すが、必要なのは公式リポジトリだけ。
  # ここで失敗しても後続の install が本当の可否を判定するので、update は落とさない。
  $SUDO apt-get update -qq || log "apt-get update に一部失敗（続行）"
  $SUDO apt-get install -y --no-install-recommends "${missing[@]}"
fi

# ── 2. 配信フロント（ui-dist）の生成 ─────────────────────────────────
# `ui/` を `ui-dist/` へ配信ビルドする。Git 管理外なので clone 直後は必ず作り直す。
# ここを飛ばすと `tauri::generate_context!` がコンパイル時に落ち、`cargo check` すら通らない。
log "ui/ を ui-dist/ へ配信ビルド"
cargo xtask build-ui

# ── 3. 依存クレートの取得 ───────────────────────────────────────────
# フック完了時点のコンテナ状態がキャッシュされるため、ここまで済ませておくと
# 以降のセッションは初回の `cargo check` から待たされない。
log "依存クレートを取得"
cargo fetch --manifest-path src-tauri/Cargo.toml --quiet

# 変数名に PICOVTUBER_ は付けない。あの接頭辞はアプリの設定キーの名前空間で、
# `doc_sync::guard` が docs/SETTINGS.md と双方向で突き合わせている。
if [ "${SKIP_PREBUILD:-}" = "1" ]; then
  log "SKIP_PREBUILD=1 のため事前ビルドは省略"
else
  log "テストバイナリを事前ビルド（初回は数分かかる）"
  # ここでの失敗はセッション開始を止めない。ビルドの可否は実際に cargo を
  # 叩いたときに判断すべきで、環境準備のフックが握り潰す話ではないため。
  if cargo test --lib --manifest-path src-tauri/Cargo.toml --no-run --quiet; then
    log "事前ビルド完了"
  else
    log "事前ビルドに失敗（環境準備は完了。cargo test の出力で原因を確認すること）"
  fi
fi

log "準備完了"
