# PicoVTuber

**1枚のイラストから、VTuber用の3Dモデル（VRM）を作って、そのまま配信する。**
生成も認識も判定も、すべてあなたのPCの中だけで動きます。クラウドAIサービスへは何も送りません。

- 作業ルール（AI/人間ともに）: [AGENTS.md](AGENTS.md)
- 仕様: [SPEC.md](SPEC.md) ／ 使い方: [MANUAL.md](MANUAL.md) ／ 全設定キー: [docs/SETTINGS.md](docs/SETTINGS.md)
- 開発・ビルド: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)

## できること

| 機能 | 内容 |
|---|---|
| モデル生成 | 立ち絵1枚から側面・背面を起こし、メッシュ化してVRM humanoidのボーンを自動で入れる |
| 表情 | 喜／怒／驚／悲／楽のプリセットに加えて、テキストで指定した表情を作る |
| リップシンク | マイク入力をPC内で解析し、口形6種（あ・い・う・え・お・ん）を実時間で当てる |
| 表情の自動切替 | ローカルAIが会話内容から話題と感情を読み、表情を切り替える |
| 配信連携 | 背景透過ウィンドウ（OBSのウィンドウキャプチャ用）と仮想カメラへ出力する |

## クラウドを使わないということ

このアプリは**外部の推論APIを一切呼びません**。イラストも、マイクの音声も、認識したテキストも、あなたのPCから出ません。

ネットワークを使うのは次の3つだけです。

1. 公式配布元からの生成モデル重み・ランタイムの取得（初回のみ）
2. アプリ本体の更新確認
3. あなたが明示的に始めたローカル配信連携（同じPC内の配信ソフトとの接続）

その代わり、生成の速度と品質は**あなたのPCの性能に左右されます**。GPUが無い環境では、機械学習を使う工程（分割・多視点生成・メッシュ化）が長時間かかるか、そもそも動きません。動かない工程は「成功」と表示せず、理由を出して止まります。

## 取り込む画像について

**自分が権利を持つ、または権利者から許諾を得たイラストだけを使ってください。** アプリは取り込み時に確認を求め、確認なしに生成へは進みません。他人の作品を集める機能（URLからの自動収集、SNSからの一括取得、権利表示の除去）は実装しません。

## 動作環境

- Windows 10 / 11、macOS 14 以降、Ubuntu Linux 24.04 LTS x64
- 機械学習を使う工程は NVIDIA GPU（CUDA）を推奨。CPUのみでも動く工程は動きますが、生成時間が大きく伸びます。
- マイク（リップシンクを使う場合）

## はじめかた

```bash
# 1. 依存の取得（初回のみ。three.js / three-vrm）
cargo xtask setup viewer

# 2. 生成モデルの重み取得
#    ※ 採用モデルのライセンス確認が済むまで自動取得は有効にしていません。
#      いまは手元に置いた重みを設定の PICOVTUBER_MODELS_DIR で指してください。
cargo xtask setup models

# 3. 開発起動
cargo xtask dev
```

製品ビルドは `cargo xtask build`（成果物は `src-tauri/target/release/bundle/`）。

Linux では次のシステムライブラリが必要です（`.claude/hooks/session-start.sh` と同じ一覧）。

```bash
sudo apt-get install -y --no-install-recommends \
  pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev librsvg2-dev \
  libayatana-appindicator3-dev libasound2-dev libclang-dev libxcb1-dev \
  libxrandr-dev libdbus-1-dev libwayland-dev libegl-dev libgbm-dev
```

## 使う流れ

1. **取り込む** — 立ち絵（透過PNG推奨、全身が入っているもの）を選び、権利の確認に答えます。
2. **生成する** — 8つの工程が順に走ります。途中で閉じても、次回起動時に未完了の工程から再開します。
3. **確認する** — できたモデルを回して見て、表情と口形を確認します。
4. **配信する** — 配信モードへ切り替え、マイクを繋いで、出力先（透過ウィンドウか仮想カメラ）を選びます。

詳しい手順は [MANUAL.md](MANUAL.md) を参照してください。

## 拡張する

生成工程も配信出力もコネクタです。**決まった場所へファイルを1つ置けば自動で登録されます。**

- 生成工程を足す → `src-tauri/src/pipeline/stages/<工程名>.rs` に `Stage` を実装して `inventory::submit!` する
- 配信出力を足す → `src-tauri/src/studio/outputs/<出力名>.rs` に `Output` を実装して `inventory::submit!` する

中央のレジストリを編集する必要はありません。インターフェースの詳細は [SPEC.md](SPEC.md)、追加手順は `.agents/skills/add-picovtuber-connector/SKILL.md` にあります。

## ライセンスと第三者資産

アプリ本体とは別に、生成モデルの重みやランタイムはそれぞれの配布元のライセンスに従います。出所と条件は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) を参照してください。**第三者のモデルを PicoVTuber のリリース資産へ再配布はしません**（利用者のPCが公式配布元から直接取得します）。
