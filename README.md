# LocalVTuberStudio

スマートフォンから現在の全テストキャラクターを確認する場合は、[キャラクター実画面ギャラリー](docs/CHARACTER_GALLERY.md)を開いてください。

イラスト1枚から、原画の見た目を保った2D/2.5D VTuberキャラクターを作るデスクトップアプリ。**クラウドAPIを一切使わず、すべてローカルで完結する。**

> **現在のアプリコードは旧3D試作で、廃止対象です。** 女性3体で工程の完走までは確認しましたが、顔・髪・衣装の見た目が不合格でした。2026-09-04に2D/2.5Dへ変更しましたが、第一候補See-throughの必須重み2件にライセンス表示がなく、正規実装は権利確認待ちです（[docs/TASKS.md](docs/TASKS.md)）。旧3Dコードと失敗生成物は新経路へ引き継ぎません。

## できること

- 手持ちのイラスト1枚から、原画品質を保った2D/2.5Dモデルを自動生成する（権利確認後に正規実装）
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

キャラクターは次の6工程で作ります。各工程の成果物を `characters/<characterId>/` 以下へ保存してから次へ進むため、失敗した工程だけを再実行できます。現在の操作画面では、①と②をまとめて `mesh` として扱い、以降を `rig`、`capture`、`expression`、`facepatch` と表示します。

| 工程 | 処理 | 主な成果物 | 次工程での用途 |
|---|---|---|---|
| 入力登録 | PNG/JPEGを取り込み、表示名・人物設定・同一性タグを記録 | `source/input.png`、`character.json` | すべての再生成の起点。入力原本は上書きしない |
| ① 背景除去 | 人物を切り出し、透過画像へ変換 | `source/isolated.png`、`model/foreground.png` | 3D生成へ背景を混入させないための入力 |
| ② 画像→3D | 透過人物を正方形へ配置し、メッシュ・UV・テクスチャを生成 | `model/reconstruction-input.png`、`model/mesh.glb`、`model/texture.png`、`model/metrics.json` | リギング対象の3D形状と基準テクスチャ |
| ③ リギング | ヒューマノイド骨格を配置し、頂点へスキニングウェイトを設定 | `model/rigged.vrm`、`model/rig-metrics.json` | 表示・アニメーション・顔領域判定に使うリグ付きモデル |
| ④ 中立キャプチャ | リグ付きモデルの正面顔を決定した画角でレンダリング | `facepatch/neutral.png`、`facepatch/capture_frame.json`、`model/thumbnail.png` | 表情生成の基準画像と、逆投影で再利用するカメラ情報 |
| ⑤ 表情生成 | 中立顔の目・眉または口だけをローカルで編集 | `facepatch/expr/eyes/<表情>.png`、`facepatch/expr/mouth/{a,i,u,e,o,close}.png`、`facepatch/expr/metrics.json` | 標準5表情・任意表情と、日本語母音5種＋閉口の直交レイヤー |
| ⑥ 逆投影 | 目・眉レイヤーと口形を組み合わせ、変化部分だけをUVへ焼き戻す | `facepatch/projected/<表情>/<口形>.png`、`facepatch/diagnostics/`、`facepatch/signature.txt` | 実行時に `rigged.vrm` へ差し替えて表示する表情別テクスチャ |

完成時に使う中心資産は `model/rigged.vrm` と `facepatch/projected/` 以下の表情テクスチャです。音声から母音を判定して口形を切り替え、会話状態から表情を選びます。`character.json` で各工程の状態を管理し、未着手は `pending` として扱い、実行後は `running / complete / failed` と理由を保存します。

```text
characters/<characterId>/
├─ character.json
├─ source/
│  ├─ input.png
│  └─ isolated.png
├─ model/
│  ├─ foreground.png
│  ├─ reconstruction-input.png
│  ├─ mesh.glb
│  ├─ texture.png
│  ├─ metrics.json
│  ├─ rigged.vrm
│  ├─ rig-metrics.json
│  └─ thumbnail.png
└─ facepatch/
   ├─ neutral.png
   ├─ capture_frame.json
   ├─ expr/
   │  ├─ eyes/<表情>.png
   │  ├─ mouth/{a,i,u,e,o,close}.png
   │  └─ metrics.json
   ├─ projected/<表情>/<口形>.png
   ├─ diagnostics/
   └─ signature.txt
```

上流工程を再実行すると、古い成果物を混ぜないようにその工程以降だけを無効化します。たとえば③をやり直すと、旧VRM、キャプチャ、表情、逆投影結果を破棄して③から作り直します。生成に失敗した場合は `character.json` を `failed` にし、壊れた成果物へ黙って進みません。

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
