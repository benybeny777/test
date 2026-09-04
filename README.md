# LocalVTuberStudio

スマートフォンから現在の全テストキャラクターを確認する場合は、[キャラクター実画面ギャラリー](docs/CHARACTER_GALLERY.md)を開いてください。

イラスト1枚から、原画の見た目を保った2D/2.5D VTuberキャラクターを作るデスクトップアプリ。**クラウドAPIを一切使わず、すべてローカルで完結する。**

> **現在は2D/2.5D経路への移行中です。** `isolate → decompose → rig2d` の工程接続、SAM 2.1候補マスク、目・口開閉元を含む10意味レイヤーのPNG・manifest・確認用PSD、`lvs-anime25d-v1`リグ骨格まで実装しています。実アクションと共通レンダラー、全テストキャラの再生成は [docs/TASKS.md](docs/TASKS.md) T11/T12で続けます。旧3D試作は品質不合格の記録として残しますが、新しい既定UIでは実行しません。

## できること

- 手持ちのイラスト1枚から、原画品質を保った2D/2.5Dモデルを自動生成する（実装中）
- 表情6種（笑顔・まばたき・怒り・悲しみ・驚き・口開き）と、日本語の五十音口形（あいうえお＋閉口）を自動生成する
- マイク音声をローカル解析してリアルタイムに口を動かす
- OBS のブラウザソースへ**透過のまま**出力する。グリーンバックもプラグインも不要
- 背景を生成する
- ローカルLLMと音声認識で AITuber として会話する

標準の紺色ボブ、長髪・装飾の薄紫、ゆったりした大袖の緑髪という女性3体で回帰確認しています。男性キャラクターは同梱候補・検証素材に含めていません。単画像から作る3D顔や小物は商用クラウド生成より粗く、入力画風によって表情品質に差が出ます。品質上の差と実測は [SPEC.md](SPEC.md) 4.10 に記録しています。

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
| ① `isolate` | 人物を切り出し、元キャンバスの透過画像へ変換 | `source/isolated.png` | レイヤー分解の入力 |
| ② `decompose` | SAM 2.1候補マスクを位置・左右・包含関係で意味部位へ割り当て | `layers/manifest.json`、`layers/parts/*.png`、`layers/source.psd` | 2.5Dリグの描画部品と確認用PSD |
| ③ `rig2d` | 部位と重なり順を検証し、製品内2.5Dリグへ変換 | `rig2d/rig.json` | T11で実装する本体・OBS共通レンダラーの入力 |

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
   └─ rig.json
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
