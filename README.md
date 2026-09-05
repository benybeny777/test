# LocalVTuberStudio

スマートフォンから現在の全テストキャラクターを確認する場合は、[キャラクター実画面ギャラリー](docs/CHARACTER_GALLERY.md)を開いてください。

イラスト1枚から、原画の見た目を保った2D/2.5D VTuberキャラクターを作るデスクトップアプリ。**クラウドAPIを一切使わず、すべてローカルで完結する。**

> **現在は2D/2.5D経路への移行中です。** `isolate → decompose → rig2d` で、原画・部位・目口差分と首/襟を分解し、schema_version 3 の `lvs-anime25d-v1` リグを生成します。テクスチャは原寸のまま透過余白だけを切り詰めます。目口の実測座標、口の2軸変形、小角度の動きを実装中です。**ひより相当の品質や機能単位の分割は未達・検証中**です。詳細は [docs/TASKS.md](docs/TASKS.md) T11/T12を参照してください。

ローカルの[原画・描画比較ページ](ui/check.html)は、`sidecar/.venv/Scripts/python.exe tools/preview_server.py` を起動して `http://127.0.0.1:8791/ui/check.html` を開きます。スマホからは信頼できるLAN内で `--lan` を付け、PCのIPアドレスの8791番へ接続します。配信対象は確認画面と検証用キャラだけです。正規pipeline-probeで生成した3キャラを切り替え、左右の閉眼・口の2軸・顔向き・顔の拡大を確認できます。GitHub上では動的プレビューは動きません。

## できること

意味解析は利用者承認に基づきGrounding DINO baseを通常の生成経路へ接続しています。`cargo xtask setup grounding`で固定版を取得し、設定画面から保存先と検出閾値を変更できます。Florence-2は比較用のままです。[比較手順](docs/DEVELOPMENT.md#意味解析候補の比較)と、未達の素材分離・品質条件は別に管理します。

目標のパイプラインは「原画/透過検査 → 構造解析 → 素材分離 → 隠れ部分補完 → 部位別リグ → 自動/目視QA」です。現在実行できる3工程とは異なり、まだ実装途中です。目口・首/襟・髪束を独立して動かせる素材を作り、ひよりを品質目標に検証します。[設計と未確定事項](SPEC.md#42-生成パイプライン)を参照してください。

- 手持ちのイラスト1枚から、原画品質を保った2D/2.5Dモデルを自動生成する（実装中）
- 左右のまばたきと、`MouthOpenY × MouthForm` による口形を確認する（品質調整中。6表情の自動生成は現行2.5Dでは未対応）
- マイク音声をローカル解析してリアルタイムに口を動かす
- 同じ描画コードをブラウザ確認画面で動かす。OBS機能・PicoAgentへの組み込みは現在の対象外
- 背景を生成する
- ローカルLLMと音声認識で AITuber として会話する

現行方式は実写・女性A・むぎを別原画で検証中です。過去の3D方式での合格を現行2.5Dの合格として扱いません。男性キャラ・後ろ姿・VRM対応は現在の対象外です。品質差は [SPEC.md](SPEC.md) 4.10 に記載します。

## 動かない・やらないこと

- **クラウドAPIを使わない。** ネットワークアクセスはモデルの初回ダウンロードだけ。生成中は完全オフライン
- **動画生成は初期スコープ外。** VRAM 8GB では実用にならないため
- **課金・アカウント登録はない**

## 必要な環境

| 項目 | 要件 |
|---|---|
| OS | Windows 10/11 x64 |
| GPU | **CUDA対応 NVIDIA GPU が必須。VRAM 8GB 以上。** GPU 非搭載機、AMD / Intel GPU では動作しません |
| RAM | 16GB 以上 |
| ディスク | 相当量が必要（実測後に確定） |
| マイク | リップシンクと音声会話に必要 |

**CPUのみでの動作には対応しません。** 生成が実用的な時間で終わらないため、中途半端に動かすより非対応を明示する方針です。

Python のインストールは不要です（必要なランタイムを同梱します）。

## 開発

Rust、Tauri CLI、WebView2 を用意し、`cargo xtask dev` で起動する。テスト一式は `cargo xtask verify`、Windows向けNSIS配布ビルドは `cargo xtask build`。Node.js は不要。

初回だけ、監査済み固定版を次の順で取得します。取得後の生成・会話・認識はローカルだけで動きます。

```powershell
cargo xtask setup comfy
cargo xtask setup sidecar
cargo xtask setup models
cargo xtask setup engines
cargo xtask dev
```

操作画面で入力イラストのパス、表示名、髪・瞳・衣装などの英語タグを登録し、「全工程を実行」を押します。失敗した場合は該当工程だけを選んで再実行できます。入力原本は上書きしません。

## キャラクター生成の処理フロー

キャラクターは次の3工程で作ります。各工程の成果物を `characters/<characterId>/` 以下へ保存してから次へ進むため、失敗した工程だけを再実行できます。

| 工程 | 処理 | 主な成果物 | 次工程での用途 |
|---|---|---|---|
| 入力登録 | PNG/JPEGを取り込み、表示名・人物設定・同一性タグを記録 | `source/input.png`、`character.json` | すべての再生成の起点。入力原本は上書きしない |
| ① `isolate` | 元キャンバスを保持。既存の透過は維持し、不透明入力は背景除去 | `source/isolated.png` | レイヤー分解の入力 |
| ② `decompose` | SAM 2.1で全身と頭部を解析し、画素から目口位置・差分を生成 | `layers/manifest.json`、`layers/parts/*.png`、`layers/source.psd` | 2.5Dリグの描画部品と確認用PSD |
| ③ `rig2d` | 部位と重なり順を検証し、製品内2.5Dリグへ変換 | `rig2d/rig.json`、`rig2d/parts/*.png` | 本体・ブラウザ確認用共通レンダラーの入力 |

中心資産は `layers/manifest.json`、`layers/parts/`、`rig2d/rig.json` です。`character.json` で各工程の状態を管理し、未着手は `pending`、実行後は `running / complete / failed` と理由を保存します。

```text
characters/<characterId>/
├─ character.json
├─ source/
│  ├─ input.png
│  └─ isolated.png
├─ layers/
│  ├─ source.psd
│  ├─ manifest.json
│  └─ parts/*.png
└─ rig2d/
   ├─ rig.json
   └─ parts/*.png
```

上流工程を再実行すると、その工程以降だけを無効化します。たとえば②をやり直すと、以前のレイヤーと2.5Dリグを破棄して②から作り直します。生成に失敗した場合は `character.json` を `failed` にし、壊れた成果物へ黙って進みません。

顔をAIで細かく立体化せず、滑らかな共通頭部へ原画の顔を投影する方式の初回試作は、[顔テンプレート投影の試作結果](docs/TEMPLATE_FACE_PROTOTYPE.md)で正面・斜め・横の実レンダーを確認できます。この試作は方式比較用であり、採用決定や完成品質を示すものではありません。

開発環境の準備、ビルド、実行の手順は [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) を参照してください。

## ドキュメント

| ファイル | 持ち場 |
|---|---|
| [AGENTS.md](AGENTS.md) | 作業エージェント向けの恒久ルール（正本） |
| [SPEC.md](SPEC.md) | アーキテクチャ・データ形式・パイプラインの現行仕様 |
| [docs/TASKS.md](docs/TASKS.md) | 未完了作業と着手順 |
| [docs/SETTINGS.md](docs/SETTINGS.md) | 全設定キー（正本） |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | 開発・ビルド・配布 |

## ライセンスと権利についての注意

- 入力するイラストは**自分が権利を持つもの、または利用許諾を得たもの**を使ってください
- 生成AIの出力に著作権法上の保護が及ぶとは限りません
- 同梱するモデルにはそれぞれのライセンスが適用されます。一覧は [SPEC.md](SPEC.md) を参照してください


### 再生成とメモリ調整

目口検出・補完方式を変えた場合は `decompose` から再実行します。旧リグをそのまま表示せず、原画を保持したまま更新してください。生成欄の「SAM同時処理点数」は既定8です。原画解像度・探索点総数を落とさず、同時処理の量だけを変えます。設定保存後、次回の分解から反映します。
