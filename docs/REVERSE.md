# 本家「VTuberオリキャラメーカー」リバース解析記録

このファイルは、クローン実装の根拠となる**本家の実測仕様**の正本とする。ここに書いてあるのは推測ではなく、配布バイナリから実際に取り出した値・アルゴリズム・データ構造である。推測が混ざる箇所は「推定」と明記する。

実装側の設計は [SPEC.md](../SPEC.md)、恒久ルールは [AGENTS.md](../AGENTS.md)、未完了作業は [TASKS.md](TASKS.md) が持ち場。このファイルは**本家の事実**だけを書き、こちらの設計判断を書かない。

## 解析の対象と方法

| 項目 | 値 |
|---|---|
| 製品名 | VTuber Original Character Maker（VTuberオリキャラメーカー） |
| 発行元 | Sumeragi（`app.info`） |
| 配布元 | `https://cdn.vtuber-ocm.com/launcher/` |
| 解析対象 | `VTuberOriginalCharacterMaker-win-Portable.zip` 664MB / 587エントリ |
| アプリ版 | v1.0.15 / Unity `6000.3.6f1` |
| 解析日 | 2026-09-02 |
| 方法 | ZIPを**展開のみ**（インストーラは実行していない）。`Assembly-CSharp.dll` を ILSpy 11.0 で逆コンパイル。同梱JSON・署名テキスト・PNGを直接読解 |

実行はしていないため、**動的な挙動（失敗時のリトライ、UIの遷移）は未確認**。静的に読み取れた範囲だけを記載する。

本記録は2系統の解析を統合している。片方は文字列抽出による受動的な静的調査、もう片方は ILSpy による逆コンパイル。**逆コンパイル由来の記述には出典として該当クラス名を明記**してある。

### 法的な線引き

**本家の利用規約はリバースエンジニアリングを原則禁止している。** 本記録の一部（アルゴリズムの詳細）は逆コンパイルによって得たものであり、規約上の争点になりうる。これを踏まえた運用は次のとおり。

- **本ファイルは実装の足場として一時的にのみ存在させ、対応する実装が終わった節から順に削除する**（[AGENTS.md](../AGENTS.md) の「リバース資料の扱い」）
- 本家の実装コード・アセット・テクスチャ・モデル・音声・プロンプト文言を**クローンへ複製しない**。解析対象のバイナリ、展開物、逆コンパイル結果を**リポジトリへコミットしない**
- 参照するのは**動作仕様とデータ構造の理解まで**とし、実装は独自に書く。アルゴリズムの考え方（幾何計算・信号処理）自体は保護対象でないため独自実装の根拠として使う
- 本家の英語プロンプト全文は本ファイルに引用せず、後述「表情プロンプトの構造」で構成要素のみ記載する

より安全側に倒す場合は、逆コンパイル由来の節（フェイスパッチの投影手順、リップシンクの内部処理）を破棄し、**外形的な観測とデータ形式だけから独自に設計し直す**選択肢もある。採否は利用者の判断に委ねる。

## 全体構成

Unity 6 系 + URP、**Mono バックエンド**（IL2CPPではないため `Assembly-CSharp.dll` が可読）。更新機構は Velopack（`Update.exe` + `current/` + `sq.version`）。Steamworks.NET を同梱しており Steam 版を準備中と推定。

主要な同梱ライブラリ:

| ライブラリ | 用途 |
|---|---|
| glTFast + Draco + Ktx | glTF/GLB のランタイム読み書き |
| TriLib Core | FBX / OBJ / DAE / PLY / STL のランタイム読み込み |
| Unity.Formats.Fbx, AloneSoft.VeryAnimation | FBX 書き出し・アニメーション編集 |
| Unity.Recorder | 動画書き出し |
| unity.webp / libwebp | WebP 入出力 |
| Mono.Data.Sqlite | ローカルDB |
| Exoa TouchCameraPro / Odin Inspector / UniTask | UI・カメラ・非同期 |

`Ariadne`（ダンジョン探索）や `BTL_*`（戦闘システム）など**本製品と無関係な旧ゲームプロジェクトのコードが大量に残存**している。流用元プロジェクトの上に構築したものと推定され、クローンで模倣する必要はない。

## クラウド依存（クローンで置き換える対象）

本家は「ローカル処理」と称しているが、**生成の中核はすべてクラウド**である。ローカルで動くのは音声認識・LLM・リップシンク・描画のみ。

`RM_CloudAituberGenerationJobTasks` が生成ジョブの工程を示す:

```
isolatedImageTaskId    → 入力イラストの背景除去・キャラ切り出し
characterSheetTaskId   → 正面・側面・背面の三面図生成
modelTaskId            → 画像→3Dメッシュ化
rigTaskId              → 自動リギング
```

`RM_CloudAituberGenerationModelOptions` の実測既定値:

```
should_texture=true, enable_pbr=true, should_remesh=true
model_type="standard", symmetry_mode="auto", topology="triangle"
target_polycount=60000, ai_model="latest"
image_enhancement=true, remove_lighting=true, moderation=false
```

| 工程 | 本家の外部サービス | 根拠 |
|---|---|---|
| 画像生成・編集（切り出し／三面図／表情／背景） | OpenAI `gpt-image-2` | `expression_pipeline_signature.txt` の `faceModel=gpt-image-2`、アセンブリ内の文字列 |
| **画像→3Dメッシュ** | **Tripo** | マルチビュー画像 → textured GLB。公式サイト記載と整合 |
| **自動リギング・アニメーション** | **Meshy** | アセンブリ内に `/openapi/v1/rigging` と `/openapi/v1/animations` が存在。`RM_MeshyClient`、`RM_CloudAituberGenerationMeshyUrls`、`"meshy"` 42箇所 |
| 動画生成 | BytePlus | 公式サイト記載 |

**3D化と リギングは別業者に分かれている。** `generated_glb.glb`（Tripo）→ `rigged_fbx.fbx`（Meshy rigging）→ `idle_fbx.fbx`（Meshy animation）という受け渡しになる。

`target_polycount=60000` と `topology=triangle` は、後段のフェイスパッチ投影が三角形単位のレイキャストで動くことと整合する。

生成ジョブの記録は `expression_sources.json` に残る。表情画像1枚ごとに独立したクラウドジョブが走る:

```jsonc
{ "records": [
  { "expressionKey": "natural__a",
    "aituberId": "character_96ebdf1f226e4122",
    "jobId": "setup_20260827135847156_d4bbjm",
    "importedAtIso": "2026-08-27T13:59:43.1805790Z" }
]}
```

タイムスタンプから読める実測値: **1画像あたり約50秒**。母音5枚はジョブIDのミリ秒が1秒以内に並んでおり**並列で投げている**。1表情セット（本体＋母音6枚）でおよそ2〜3分。

## ローカルで動いている部分

`StreamingAssets/LocalAI/` に llama.cpp と whisper.cpp のバイナリを同梱している。`seed_manifest.txt` の版表記:

```
llama-b10080_whisper-v1.9.1_binonly_universal_macos14_v4
```

- **LLM**: llama.cpp b10080（`llama-server` + `mtmd`（マルチモーダル）同梱）。win-x64 / mac-arm64 / mac-x64 の3系統。CPU 命令セット別 DLL を10種以上同梱し実行時に選択（sse42 / sandybridge / ivybridge / haswell / alderlake / icelake / skylakex / cascadelake / cooperlake / cannonlake / sapphirerapids / piledriver / zen4）。
- **STT**: whisper.cpp v1.9.1（`whisper-server`）に加え、**`parakeet.dll` / `parakeet-cli.exe` を同梱**。NVIDIA Parakeet 系の実装と推定。
- **CUDA版は同梱されていない**。GPU 用バイナリが一切無く、ローカル推論は CPU 前提。

モデル本体（GGUF）はバイナリに含まれず、初回起動時に約1.5GBをダウンロードする（`RM_LocalAiModelDownloader` / `RM_LocalAiAssetSeeder`）。アセンブリ内の文字列から**モデル名を確定できた**:

| 用途 | モデル | 備考 |
|---|---|---|
| 会話・判断LLM | `qwen2.5-1.5b-instruct-q4_k_m.gguf` | 1.5B の4bit量子化。CPUで回る規模に絞っている |
| STT | `ggml-small.bin` | 既定。`ggml-tiny.bin` も文字列として存在し、軽量側の選択肢と推定 |

**LLM の役割は会話生成だけではない。** 発話テキストを受けて「どの表情を出すか・どの動画を再生するか」を**JSONで選ばせる**分類器として使っている。1.5B という小ささはこの用途に合わせた選定と読める。クローンでも、表情選択は生成ではなく**選択タスク**として設計するのが妥当。

## キャラクターのデータ構造

`StreamingAssets/PresetCharacter/` に完成品が1体入っており、これが保存形式の実例になる。

```
preset_character.json          キャラ定義
preset_manifest.txt            ファイル一覧＋サイズ（整合性検証用。TSV、1行目 "PresetCharacterSeedManifest\t1"）
model/source.png               入力イラスト           1024x1024 RGBA
model/thumbnail.png            サムネイル             1024x1024 RGBA
model/generated_glb.glb        生成された素メッシュ    7.4MB
model/rigged_fbx.fbx           リグ付きメッシュ       11.8MB  ← 実行時に使うのはこれ
model/idle_fbx.fbx             アイドルアニメーション  11.3MB
facepatch/                     表情システム一式
video/*.mp4 + *_thumb.png      生成済み動画
```

`preset_character.json` の構造（実測）:

```jsonc
{
  "version": "20260827141206",          // manifest と対応する世代
  "label": "お試しキャラ",
  "characterSettings": "",              // AITuber用の人格設定（自由記述）
  "modelFileName": "rigged_fbx.fbx",
  "idleFileName": "idle_fbx.fbx",
  "generatedGlbFileName": "generated_glb.glb",
  "thumbnailFileName": "thumbnail.png",
  "sourceImageFileName": "source.png",
  "videos": [{
    "id": "streaming_video_20260823125428668",
    "fileName": "...mp4", "thumbnailFileName": "..._thumb.png",
    "label": "動画 02",
    "motionPrompt": "ぴょんぴょんジャンプする",
    "usageId": "custom", "usageLabel": "それ以外", "usageCustomText": "待機",
    "durationSec": 5,
    "transparentBackground": true,
    "lipSync": false
  }],
  "framings": [{
    "presetId": "green_screen", "presetLabel": "グリーンバック",
    "upperBodyVerticalBias": 0.0, "upperBodyHorizontalBias": 0.0, "upperBodyScale": 1.0,
    "characterYaw": 1.9565, "characterPitch": -15.4076,
    "armPoseLeftUpperArm":  {"x":  8.1356, "y": 16.9492, "z": -1.3559},
    "armPoseLeftLowerArm":  {"x":  0.0,    "y":  0.0,    "z":  4.7458},
    "armPoseRightUpperArm": {"x": -0.6780, "y":  5.7627, "z":  5.7627},
    "armPoseRightLowerArm": {"x":  0.0,    "y":  0.0,    "z":  0.0},
    "armPoseHead":          {"x":  0.0,    "y":  0.0,    "z":  0.0}
  }]
}
```

背景プリセットの実測ID: `green_screen` / `cozy_room` / `kids_room` / `gaming_room` / `cafe` / `night_window` / `simple_studio`。

`framings` は背景プリセットごとに構図を持つが、実測データでは**同じ `presetId` が重複して現れ、値も全件同一**。配列インデックスで引いていて重複排除していないと推定され、この設計は模倣しない。

腕ポーズをオイラー角で個別に持つのは、生成された素体が **T ポーズ / A ポーズで出てくるため、配信映えする自然な腕位置へ実行時に補正する**必要があるから。`RM_RuntimeArmPoseAdjuster` / `RM_RuntimeArmPoseCorrector` が対応する。

## フェイスパッチ表情システム（本家の中核。最重要）

本家の「イラスト1枚から表情豊かな3Dモデル」を成立させている仕組み。**ブレンドシェイプを一切使わず、顔だけを2D画像として編集し、テクスチャへ投影し直す。**

AI生成メッシュにはブレンドシェイプが無く、UV展開も自動生成で数千の微小アイランドに分割される（実測: 2048x2048 のアトラスが断片だらけ）。そこへ手を入れずに表情を付けるための解法がこれである。

### 工程

```
1. 顔正面を正射影で 1024x1024 キャプチャ        → neutral.png
2. その画像を画像編集AIへ渡し表情差分を作る     → <expr>.png            (1024x1024)
   同じ要領で母音 a/i/u/e/o と close も作る     → <expr>__a.png ...
3. 編集画像をメッシュのUVアトラスへ逆投影        → projected_v20_<expr>.png (2048x2048)
4. 実行時はマテリアルのテクスチャを差し替えるだけ
```

工程3までは**生成時に一度だけ**行い、結果をディスクにキャッシュする。実行時の表情切り替え・リップシンクは**テクスチャ差し替えのみ**なので極めて軽い。

### キャプチャフレーム（`capture_projection_frame.json` 実測値）

```jsonc
{
  "version": "projection_frame_v1",
  "captureResolution": 1024,
  "facingFlipped": false,
  "patchCenterLocal":  { "x": 0.004807, "y": 1.530806, "z": 0.062750 },
  "patchForwardLocal": { "x": 3.09975e-8, "y": 0.0, "z": 1.0 },
  "patchUpLocal":      { "x": 0.013936, "y": 0.999903, "z": -4.31993e-10 },
  "patchSize":  { "x": 0.370299, "y": 0.330638 },
  "patchFrontZ": 0.173750,
  "patchBackZ":  0.173750,
  "bindPoseFrame": false
}
```

顔を含む矩形をモデルローカル座標で定義している。中心が y=1.53 にあることから、生成モデルは**おおむね身長1.7前後・足元が原点のスケールに正規化されている**と読める。奥行きは前後 ±0.174 の窓。

キャプチャカメラの視野は実測コードで `max(patchSize.x, patchSize.y) * 0.62 * 2` = 約 0.459 の正射影サイズ。パッチ矩形の約1.24倍を写しており、顔の周囲に余白を取っている。

同じ値が `capture_pose_signature.txt` に1行の署名として保存され、**リグやポーズが変わったらキャッシュを破棄する**ための照合キーになっている:

```
capture_pose_v4|renderer=.../rigged_fbx/char1|1024|flip=0|center=...|forward=...|up=...|size=...|front=...|back=...
```

### 投影パラメータ（`expression_pipeline_signature.txt` 実測値）

パイプライン版は `expression_pipeline_v20`。パラメータが平文で全部入っている:

| キー | 実測値 | 意味 |
|---|---|---|
| `faceModel` | `gpt-image-2` | 表情差分を作る画像編集モデル |
| `projectionTextureSize` | `2048x2048` | 投影先アトラス解像度 |
| `projectionMaterial` | `RuntimePreviewGltfMaterial_FlatUnlit` | **アンリット**マテリアル |
| `projectionUv` | `1.000,1.000,0.000,0.000` | UV スケール・オフセット |
| `diff` | `0.075` | 中立画像との差分がこの閾値超のみ書き込む |
| `alpha` | `0.030` | この α 未満は対象外 |
| `seamPadding` | `4` | UV継ぎ目を4px膨張させて隙間を埋める |
| `faceMask` | `True` | 顔マスクを使う |
| `projMask` | `0.500,0.580,0.240,0.260` | 顔マスク楕円 中心(cx,cy)・半径(rx,ry)、正規化座標 |
| `projRegion` | `0.420,0.460,0.340,0.520,0.180` | 3D側の採用領域 |
| `projNormal` | `0.350,0.180` | 法線がカメラを向いている度合いの閾値 |
| `projHits` | `3,0.050` | レイキャスト最大ヒット数3・深度窓0.05（**手前の面だけ採用し後頭部への貫通を防ぐ**） |
| `align` | `True` | 編集画像を中立画像へ位置合わせ |
| `colorMatch` | `True` | 色味合わせ |
| `neutralAlpha` | `True` | 中立のαを使う |
| `featureMask` | `0.820,0.740` | 目・口など特徴部の重み |
| `shell` / `shellCore` | `False` / `True` | 顔だけの別メッシュ（シェル）方式。既定では未使用 |
| `flattenI2I` | `True` | image-to-image 前に平坦化 |
| `applyMode` | `1` | 適用モード |
| `brush` | `0` | 手動ブラシ無効 |
| `opaque` | `False` | 不透明化しない |

`shellDepth=0.0040,0.360,0.220,0.0015,0.550,0.0080` / `shellUv=1.000,1.000,0.500,0.500` / `shellMask=0.500,0.470,0.190,0.180` はシェル方式用で、既定では使われない。

### 投影アルゴリズム（`RM_FacePatchController.BakeExpressionToProjectionTexture` 実測）

逆コンパイルから読み取った処理順:

1. 元アトラス（`_originalProjectionTextureReadable`）を複製する。以降ここへ上書きする。
2. `SkinnedMeshRenderer` を **`BakeMesh` で現在ポーズのメッシュへ焼く**。中立時のスナップショット `_neutralBakedMesh` があればそれを使う（ポーズが動いても投影がぶれない）。
3. `headBone` のインデックスを `renderer.bones` から引く。**見つからなければ処理を中止**する。
4. `BuildAllowedProjectionTriangles` — 三角形を絞り込む。判定は
   - 頂点のボーンウェイトが **頭ボーンに `projectionTriangleHeadWeightThreshold` 以上**乗っていること（＝体や髪の一部を除外）
   - 法線がキャプチャ方向を向いていること（`projNormal`）
   - パッチ矩形の内側（`projRegion`）
5. `BuildSourceContentMask` — 編集画像側のマスクを作る。**α が `alpha` 閾値以下、または「ほぼ白の背景」の画素を除外**する（`IsNearlyWhiteBackground`）。
6. `RestrictSourceSelectionToHead` — 頭部の縦位置（`TryGetHeadOriginNormalizedV` / `TryGetHeadBottomNormalizedV`）でさらに首から下を切る。
7. `RasterizeProjectionTrianglesToTexture` — 採用三角形を **UV空間でラスタライズ**し、各テクセルについて
   - そのテクセルに対応する3D座標を求める
   - キャプチャカメラへ投影して編集画像のUVを得る
   - `MeshCollider` へレイキャストし、**最大 `projHits`=3 ヒットの中で深度窓 0.05 内の最前面**だけ採用（`ProjectRayOntoProjectionTexture`）
   - 編集画像の画素をアトラスへ書き込む
8. `PadProjectedTextureSeams` — 書き込み済み画素の周囲 `seamPadding`=4px を膨張させ、UV継ぎ目の隙間を埋める。**非採用三角形のUVマスクを避けて**膨張させるので他パーツへ滲まない。
9. 投影画素が0なら失敗としてデバッグ画像を保存して中止。

投影結果は `projected_v20_*.png` として保存され、次回以降は再計算しない。パイプライン署名が変われば `v20` が上がりキャッシュが無効になる設計。

### 表情の種類

組み込み6種（`expression_pipeline_signature.txt` にキーとプロンプトが平文で存在）:

```
smile / blink / angry / sad / surprised / mouth_open
```

母音は `natural__a` `__i` `__u` `__e` `__o` `__close` の6枚を各表情ごとに持つ。つまり**1表情あたり画像7枚**（表情本体＋母音6）を生成しており、公式の「表情セット 200pt」はこの枚数分の課金と整合する。

ユーザー定義表情は `expression_library.json` に日本語プロンプトごと保存される（実測3件）:

```jsonc
{ "customExpressions": [
  { "key": "custom_怒り_b3efcc8b_20260812051757445", "label": "怒り",
    "prompt": "眉を少し寄せ、視線を強めた控えめな怒り表情。",
    "isBuiltIn": false, "createdAtIso": "2026-08-12T05:17:57.4456710Z", "updatedAtIso": "..." }
]}
```

キーの形式は `custom_<ラベル>_<8桁hash>_<yyyyMMddHHmmssfff>`。**日本語をそのままファイル名に使っている**ためエンコーディング事故が起きやすく、クローンでは避ける。

### 表情プロンプトの構造

本家の英語プロンプトは、以下の要素をこの順で並べた1文＋指示文で構成されている（文言自体は引用しない）:

1. 「この入力キャラクター顔画像を編集せよ」
2. **維持すべき属性の列挙** — キャラクター同一性 / ポーズ / フレーミング / カメラ角度 / **左右の向き** / 髪型 / アクセサリ / 色 / ライティング / アニメ調3Dレンダリング
3. **「ミラーリングするな」の明示**
4. 「表情だけを変えよ」＋当該表情の具体的記述

2と3が重い。画像編集モデルは左右反転や画風ドリフトを起こしやすく、それが起きると投影時に顔が破綻するため。**ローカルモデルで置き換える場合はこの制約がさらに厳しくなる**（[SPEC.md](../SPEC.md) のリスク節を参照）。

## 「自由な動作」の正体 — 透過動画オーバーレイ

本家が「動画」と呼んでいる機能は、**3Dモデルを骨アニメーションで動かしているのではない**。キャラ画像から生成した**短い実写風動画を、3D画面の上へクロマキー合成で重ねている**。

実測（`preset_character.json` の `videos` と同梱MP4）:

| 項目 | 値 |
|---|---|
| 解像度 | 1280x720 |
| フレームレート | 24fps |
| 長さ | 5秒 |
| コーデック | H.264 |
| 透過 | `transparentBackground: true`。H.264はαを持てないため**クロマキーで抜いている** |
| 用途 | `motionPrompt`（例「ぴょんぴょんジャンプする」）＋ `usageId` / `usageCustomText`（例「待機」）で再生条件を持つ |
| リップシンク | `lipSync: false`。動画再生中は口パクを止められる |

再生する動画は LLM が選ぶ（前述）。関連クラスは `RM_ChromaKeySettings` / `RM_ChromaKeyCheckView` / `RM_VideoThumbnailCapture` / `RM_ObsFrameComposer`。

**これは短期間で「動く」を成立させるための割り切り**である。生成3Dモデルは自動リグの品質が低くフルボディモーションに耐えないため、動きの表現を動画へ逃がしている。長さ5秒・24fps という数字も、生成コスト（85pt/秒）と品質の妥協点。

クローンにとって重要なのは、**この合成レイヤー自体は動画生成AIと独立している**こと。透過動画を重ねる仕組みだけなら実装は安価で、素材は利用者が持ち込んでもよい。動画生成そのものはローカルでは重いが、**合成機能だけ先に作る**という分離が可能。

## 課金（クローンでは実装しない）

`RM_PointWallet` / `RM_BillingService` / `RM_SteamShopController`。3Dキャラ800pt、表情セット200pt、背景60pt、動画85pt/秒。認証は AWS Cognito（`RM_CognitoAuthSession`）、Steam 連携あり。ローカル完結のクローンでは不要。

## クローン実装で再現すべきもの／しないもの

| 本家の要素 | 方針 |
|---|---|
| フェイスパッチ投影方式 | **再現する。** 本家の核であり、AI生成メッシュに表情を付ける現実解 |
| キャプチャ→編集→逆投影の3段キャッシュ | **再現する。** 実行時コストをテクスチャ差し替えのみに落とす設計が正しい |
| 署名によるキャッシュ無効化 | **再現する。** ポーズやパイプライン変更で破綻するのを防ぐ |
| フォルマント最近傍リップシンク | **再現する。** 軽量で十分な品質。ただし DFT は FFT へ置き換える |
| 母音6枚×表情の事前生成 | **再現する** |
| OBS 連携 | **方式を変える。** MJPEG ではなくアルファ付きで出す（[SPEC.md](../SPEC.md)） |
| グリーンバック前提 | **再現しない。** 上記により不要 |
| 日本語をファイル名に使うキー | **再現しない。** ASCII キー＋表示名を分離する |
| `framings` の presetId 重複 | **再現しない。** キー引きにする |
| クラウド生成（画像・3D・リグ・動画） | **すべてローカルへ置換**（[SPEC.md](../SPEC.md)） |
| 課金・認証・Steam | **実装しない** |
| 旧ゲームプロジェクトの残骸 | **持ち込まない** |

## 未確認事項

静的解析だけでは確定できなかったもの。実装中に判断が必要になったら、ここへ結論を追記する。

- 初回DLされる GGUF モデルの具体名とサイズ（`RM_LocalAiModelDownloader` はURLを実行時に取得する）
- `parakeet.dll` を実際に使っているか、whisper.cpp との切り替え条件
- 生成1体あたりの実測所要時間
- 生成失敗時のリトライ回数・課金の巻き戻し挙動
- `applyMode=1` の他の値が何を指すか
- シェル方式（`shell=True`）が有効になる条件
- `idle_fbx.fbx` のアニメーションが Meshy のプリセットか生成物か
