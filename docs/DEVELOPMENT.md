# DEVELOPMENT.md — 開発・ビルド・配布

Rust と Tauri CLI だけで開発起動・テスト・Windows配布ビルドを行う。Node.js は不要。

## 必要なもの

### 原寸閉眼のローカル編集診断

`tools/evaluate-eye-inpaint.py`は採用済みAnimagine XL 4.0の固定SHAを検査して使う比較専用ツールであり、製品パイプラインへ候補を昇格しない。作業用Pythonから`--character <キャラフォルダ> --comfy <本アプリ管理ComfyUI> --output temp/<新規診断名>`を指定する。任意の`--denoise`、`--mask-grow`で条件比較する。既存利用者環境や`extra_model_paths.yaml`のある環境を使わない。

元キャンバスへVAE倍数の余白だけを足し、リサイズせず目マスク内を編集する。出力は診断領域のcandidate.png/report.json/workflow.jsonとログのみで、原画・正規素材を変更しない。起動するComfyUIは127.0.0.1の専用ポート、API/カスタムノード無効、処理後に終了・waitする。同時にほかのGPU処理を走らせない。成功ログを品質合格と解釈せず、虹彩色の閉眼への混入などを原画と目視比較する。

### 意味解析候補の比較

`tools/semantic-eval/inspect-components.py`は正規解析で保存した衣服の検出矩形を同じSAMへ渡し、最大連結領域と全領域を比較する。原寸マスクと上位の成分画像・面積を`temp/clothing-components/`へ保存する。左右に離れた衣服をノイズとして捨てていないかを確認する診断であり、その画像を製品へコピーしない。

正規出力の透明度保持は `sidecar/.venv/Scripts/python.exe tools/verify-scene-alpha.py temp/t7-characters/<ID> ...` で検査する。全キャンバスの `scene_*` 部位をsource-over合成し、背景除去原画との差があれば失敗する。これは原寸アルファ検査であり、RGB同一性・画面のフィルタリング・動作時の品質は別途確認する。

`segment.py --roles eyes --box-context 0.5`は、検出矩形の各辺へ幅/高さの50%を足した原寸ROIを同じSAMへ入力する局所解析の比較である。出力を`box-context-0.5/`へ分離し、sampling_regionを記録する。候補矩形や元画像を変更せず、モデル内部の解析解像度と最終素材の原寸を混同しない。全頭部解析と原寸マスクを比較し、背景/髪の混入も調べてから通常経路への採否を判断する。

細部比較では`tools/semantic-eval/evaluate.py dino --model-path models/grounding-dino-base --run-name <診断名> --labels eyebrow "eye pupil" --view head --measured-head`を使える。`--measured-head`は正規解析の顔座標を参照し、頭部比率の固定切り出しを使わない。候補の語句・座標・スコア・クロップ・所要時間を診断JSONに残す。`segment.py --run-name <同じ診断名> --roles eyebrow "eye pupil"`で既存SAM2へ渡す比較ができる。左右の細部を確定できない場合は明示失敗にし、未検出の原画を成功扱いしない。これらは比較専用で、検出候補を製品リグへ自動採用しない。

通常経路のブラウザ検証はリポジトリルートで `sidecar/.venv/Scripts/python.exe tools/preview_server.py` を起動し、`http://127.0.0.1:8791/ui/check.html` を開く。原画と本体共通レンダラーを比較する。`--lan`は信頼できるLANでのスマホ確認に限る。サーバーは画面・共通描画JS・検証用キャラの画像/JSONだけを許可し、モデル・プロジェクト文書・ディレクトリ一覧を返さない。応答はno-storeとし、旧モジュールのキャッシュがある場合は版付きURLで開き直す。GPU生成後にまとめて確認し、使い終わった自分のサーバーだけを停止する。

SAM2の画像マスク推論は共有環境のSam2VideoModelを重み整合性検査付きで使う。Transformers 4.57.6の単フレームbox+points併用にはnum_objects未初期化の問題があるため、髪の矩形を公式VideoProcessorと同じ角ラベル2/3へ変換し、目口の除外点0と同じ入力へまとめる。重みや共有ライブラリを改変せず、複数マスク推論を維持する。

利用者が比較を承認したFlorence-2-large-ftとGrounding DINO baseを `tools/semantic-eval/` で再現する。比較後にGrounding DINOの組み込みが承認され、通常経路は `cargo xtask setup grounding`（`setup models`にも含む）で `models/grounding-dino-base` へ固定版・SHA256検証付きで取得する。比較用の保存先とは分離し、診断画像を製品成果物へ転用しない。共有Python 3.12/Transformers 4.57.6を使い、Florence比較に必要なtimm 1.0.29（Apache-2.0）だけを追加する。既存torch等を更新しない。

```powershell
uv pip install --python sidecar/.venv/Scripts/python.exe --no-deps -r tools/semantic-eval/requirements.txt
sidecar/.venv/Scripts/python.exe tools/semantic-eval/download_models.py
sidecar/.venv/Scripts/python.exe tools/semantic-eval/evaluate.py florence
sidecar/.venv/Scripts/python.exe tools/semantic-eval/evaluate.py dino
sidecar/.venv/Scripts/python.exe tools/semantic-eval/segment.py --backend Florence-2-large-ft
sidecar/.venv/Scripts/python.exe tools/semantic-eval/segment.py --backend grounding-dino-base
sidecar/.venv/Scripts/python.exe tools/semantic-eval/summarize.py
```

GPU工程は逐次実行する。原画は `temp/t7-characters/<id>/source/isolated.png`。evaluate/segmentの `--characters <id> ...` で別原画にも同じ処理を適用する。既定3件は比較fixtureであり、キャラ別ロジックではない。`evaluate.py --run-name repeat` を付けて反復し、推論結果の一致を集計する。モデルは `models/semantic-evaluation/`、取得manifestと画像・数値結果は `temp/` 内へ保存する。描画確認用の縮小画像を高精細素材へ採用しない。

| 比較モデル | 固定リビジョン | 重みSHA-256 |
|---|---|---|
| Florence-2-large-ft | `4a12a2b54b7016a48a22037fbd62da90cd566f2a` | `8b4e610c952eef90a836c56cda0f398a672a3a6ca7b4d96b0e09a86dee42e2c3` |
| Grounding DINO base | `12bdfa3120f3e7ec7b434d90674b3396eccf88eb` | `5548f844c928c4b6f411fa8cbcc2bfa8dbbba437cb1d513975519f93c2a9ed21` |

Florenceの公式実装は旧KVキャッシュ形式のため、比較では `use_cache=False` を明示する。重みを変えず速度の代償を受け入れる。SAM2の比較は設定に一致する `Sam2VideoModel` の単一フレーム経路を使い、重みキーの不一致を拒否する。内部APIへの依存は固定Transformers版限定であり、製品採用時には正規アダプターとテストが必要。結果は部位候補で、独立した上下唇・瞳・白目・隠れ領域の完成や動作合格を意味しない。

### 通常の開発環境

| 項目 | 版・備考 |
|---|---|
| Rust（cargo） | 安定版 latest |
| Tauri CLI | `cargo install tauri-cli --version '^2'` |
| WebView2 ランタイム | Windows のみ。多くの環境で導入済み |
| **CUDA 対応 NVIDIA GPU** | **必須。VRAM 8GB 以上。** CPU フォールバックは実装しない |
| uv | Python 3.12.13の開発用単一環境を構築するために使用。製品利用者には要求しない |

**Node.js はアプリとビルドの依存にしない。** 作業用ツール（スクリーンショット撮影など）としての利用は可。判断基準は「アプリのビルド・起動・配布に Node が要るようになるか」（[AGENTS.md](../AGENTS.md)）。

**Python は利用者側では不要。** ランタイムを同梱する。ただし**同梱する Python 環境は1つだけ**で、ComfyUI・画像→3D・リギングが同じ Python 3.12 環境を共有する（[SPEC.md](../SPEC.md) 2 方針5）。**用途ごとに環境を分けない。** 開発時は `cargo xtask setup sidecar` が用意する。

依存が衝突したら環境を増やすのではなく版を揃えて解決する。解決できない場合だけ理由を明記して利用者へ相談する。

## ComfyUI（同梱）

画像生成・編集のバックエンドは ComfyUI（[SPEC.md](../SPEC.md) 4.7.2）。開発時も**同梱版を使い、開発機の手元 ComfyUI に依存しない**。手元環境で動いて利用者環境で動かない、が最も起きやすい失敗。

- ComfyUI は **v0.34.0** を固定する。GPL-3.0 の本文、著作権表示、対応ソースの提供方法を配布物へ含める
- `cargo xtask setup comfy` が固定タグを取得し、コミットIDまで照合する。初期構成は標準ノードだけを使う
- カスタムノードは現時点では同梱しない。追加する場合は固定コミット、ライセンス、重み、直接・推移Python依存を監査し、自動更新しない
- ワークフロー JSON はリポジトリで管理する
- ポートは利用者の既存 ComfyUI（既定 8188）と衝突させない
- **常駐 ComfyUI と単発の画像→3D生成を同時に走らせない。** VRAM を取り合う（[SPEC.md](../SPEC.md) 3.1）

### T0 ライセンス監査で固定した取得元

下表のリビジョンとSHA-256はライセンス監査時点の固定値。ファイル本体はリポジトリへ入れず、セットアップ実装時に取得してSHA-256を照合する。候補を更新する場合はライセンス監査もやり直す。

| 対象 | 固定版・リビジョン | ファイルとSHA-256 |
|---|---|---|
| ComfyUI | `v0.34.0` | Gitタグを固定。Python 3.12/CUDA/PyTorchを含む全依存はT1/T2でlockfile化して別途ハッシュを固定 |
| rembg | `v2.0.83` | パッケージのwheelハッシュはT5のlockfileで固定 |
| isnet-anime | `skytnt/anime-seg@493cb60893f47441b26ec4fb9a306bce9e342982` | `isnetis.onnx`: `f15622d853e8260172812b657053460e20806f04b9e05147d49af7bed31a6e99` |
| SAM 2.1 Hiera Tiny | `facebook/sam2.1-hiera-tiny@de431c4043854a71d8101e17995dfe596bf101a5` | `model.safetensors`: `48c14467e5cf9e51870511feb72c89688e82dd74523142c0538b663e193ac2a7`。設定3ファイルも個別にSHA-256固定 |
| TripoSR | コード `107cefdc244c39106fa830359024f6a2f1c78871`、重み `stabilityai/TripoSR@5b521936b01fbe1890f6f9baed0254ab6351c04a` | `model.ckpt`: `429e2c6b22a0923967459de24d67f05962b235f79cde6b032aa7ed2ffcd970ee` |
| DINO ViT-B/16設定 | `facebook/dino-vitb16@f205d5d8e640a89a2b8ef0369670dfc37cc07fc2` | `config.json`: `b87c0270b97db085fd82cf114a761fd0f62ae7914fbd407c752a2260646b689c`。重みはTripoSR checkpoint内 |
| TRELLIS（条件付き代替） | コード `442aa1e1afb9014e80681d3bf604e8d728a86ee7`、重み `microsoft/TRELLIS-image-large@25e0d31ffbebe4b5a97464dd851910efc3002d96` | 複数ファイル構成。採用時に全manifestを固定し、`diffoctreerast` は取得しない |
| Animagine XL 4.0 Opt | `cagliostrolab/animagine-xl-4.0@2b7c1b397761bf5bd3cc42e5b39ec99314a75a96` | `animagine-xl-4.0-opt.safetensors`: `6327eca98bfb6538dd7a4edce22484a1bbc57a8cff6b11d075d40da1afb847ac` |
| ControlNet Canny SDXL | `diffusers/controlnet-canny-sdxl-1.0@eb115a19a10d14909256db740ed109532ab1483c` | `diffusion_pytorch_model.safetensors`: `ea99040544a999f814fd854575a3aee069a005d026864c8d321b82576706a221` |
| IP-Adapter SDXL Plus Face | `h94/IP-Adapter@018e402774aeeddd60609b4ecdb7e298259dc729` | adapter: `677ad8860204f7d0bfba12d29e6c31ded9beefdf3e4bbd102518357d31a292c1`、image encoder: `657723e09f46a7c3957df651601029f66b1748afb12b419816330f16ed45d64d` |
| Qwen-Image-Edit | `Qwen/Qwen-Image-Edit@ac7f9318f633fc4b5778c59367c8128225f1e3de` | 複数ファイル構成。T2で採用する場合だけ全manifestを固定 |
| llama.cpp + Qwen2.5 1.5B | llama.cpp `v0.3.0`、`Qwen/Qwen2.5-1.5B-Instruct-GGUF@91cad51170dc346986eccefdc2dd33a9da36ead9` | `qwen2.5-1.5b-instruct-q4_k_m.gguf`: `6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e` |
| whisper.cpp + Whisper small | whisper.cpp `b4938`、`ggerganov/whisper.cpp@5359861c739e955e79d9a303bcbc70fb988958b1` | `ggml-small.bin`: `1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b` |

PowerShell を使う場合は **PowerShell 7 の `pwsh`** を既定にする。見つからなければ Windows PowerShell 5.1 へ黙って降格せず、導入が必要な理由を利用者へ伝える。

## リポジトリ構成

構成の詳細と各層の責務は [SPEC.md](../SPEC.md) 4.1 を参照。

```
src-tauri/   Rust本体（設定・パイプライン・フェイスパッチ・リップシンク・配信・サイドカー制御）
ui/          操作UI（webview）。ui/shared/ に three.js 描画コードを置く
ui-stream/   OBSブラウザソース用ページ（UIなし・透過）。ui/shared/ を共有する
sidecar/     Python 3.12（背景・表情、画像→3D、リギング）
docs/        ドキュメント
xtask/       開発タスク
temp/        一時作成物のみ。.gitignore 済み
```

## コマンド

| コマンド | 用途 |
|---|---|
| `cargo xtask setup engines` | llama.cpp b10621 / whisper.cpp b4938 とQwen・Whisperモデルを取得・SHA-256検証。固定版が揃っていれば再取得しない |
| `cargo xtask setup comfy` | 同梱 ComfyUI 本体・ワークフローと、監査済みの場合だけカスタムノード固定版を用意 |
| `cargo xtask setup sidecar` | Python 3.12 ランタイムと依存を用意（**CUDA wheel は対応GPU検出時のみ**） |
| `cargo xtask setup models` | モデルを取得 |
| `cargo xtask setup sam2` | SAM 2.1 Hiera Tinyだけを固定リビジョンから取得し、全4ファイルのSHA-256を検証 |
| `cargo xtask dev` | 開発起動 |
| `cargo xtask build` | 配布ビルド |
| `cargo xtask verify` | 書式・静的解析・テスト・文書同期をまとめて実行 |
| `cargo xtask facepatch --model <vrm/glb> --neutral <png> --layered-expression-dir <11枚のディレクトリ> --atlas <png> --frame <json> --output-dir <dir> --diagnostics <dir>` | 目・眉N枚と共通口形6枚を合成し、中立スキニング済みメッシュへ逆投影 |
| `cargo xtask expression-import --neutral <png> --input <png> --output <dir> --kind <eyes/mouth> --key <ASCIIキー>` | 外部表情画像の位置・反転・色差を検査し、中立画像と署名を保存 |
| `cargo xtask mesh --input <png> --output <dir>` | anime-segで背景除去し、TripoSRで2048² UVアトラス付きGLBを単発生成 |
| `cargo xtask rig --input <glb> --output <dir> --name <表示名>` | A/Tポーズを検査し、19ボーンとheat diffusionウェイトを持つVRMを単発生成 |
| `cargo run -p local-vtuber-studio --bin lipsync-probe` | 既定マイクを3秒だけ16kHzへ変換し、FFT判定窓を検査して停止 |
| `cargo run -p local-vtuber-studio --bin stream-probe` | 透過OBSページを58090〜58099の空きポートで30秒配信し、女性3体の状態を切替 |
| `cargo run -p local-vtuber-studio --bin pipeline-probe -- <input.png> <id> <identity-tags>` | 開発用にアプリと同じRustパイプラインをヘッドレス完走 |
| `cargo run -p local-vtuber-studio --bin pipeline-probe -- --only <characterId> <stage>` | 既存キャラの選択工程だけを再実行 |
| `cargo run -p local-vtuber-studio --bin pipeline-probe -- --background <characterId> <backgroundId> "<prompt>"` | アプリと同じComfyUI管理経路でローカル背景を実生成 |
| `cargo run -p local-vtuber-studio --bin engine-probe -- voice <16kHz.wav>` | whisper.cpp→llama.cpp→表情JSON選択を連結確認 |

T2 の表情生成を試す場合は、次の順で一度だけセットアップする。

```powershell
cargo xtask setup comfy
cargo xtask setup sidecar
cargo xtask setup models
cargo xtask expression --input temp/input.png --output temp/expressions --identity-tags "髪・瞳・衣装・アクセサリの英語タグ"
```

`expression` の入力は 1024x1024 RGBA、出力は目・眉5 PNG、任意表情N PNG、共通口形6 PNGおよび `metrics.json`。ComfyUIは `127.0.0.1:58120` のみで起動し、処理後は必ず終了してハンドルを回収する。初回起動の実測が180秒を超えたため、起動待ちは600秒とする。画像生成は denoise 0.65、閉眼だけ0.85を用いる。目と口を重ならない限定マスクへ分離し、逆投影直前に組み合わせる。

背景生成は `workflows/background-txt2img-api.json` の標準ノードだけを使い、1344x756を直接生成する。補間拡大しない。人物を負のプロンプトで除外し、出力は `characters/<id>/backgrounds/<bgId>.png` へ原子的に置換する。

外部画像は1024x1024で、中立キャプチャと同じ画角・向き・背景にする。`expression-import` は中立画像も `neutral.png` として書き出し、位置ずれ12px超、左右反転の可能性、平均色差0.08超を警告する。警告は拒否ではないが、確認せず投影すると破綻し得る。投入画素はSHA-256キャッシュ署名へ含まれる。

`setup sidecar` は `nvidia-smi` でCUDA対応GPUを確認してから、Python 3.12.13とハッシュ固定済み依存を単一環境へ同期する。`setup models` はT0で固定したリビジョンから取得し、SHA-256不一致なら採用せず中間ファイルを削除する。CPUフォールバックはない。

`mesh` は入力原本を `source/input.png` に保存し、`foreground.png`、`reconstruction-input.png`、`texture.png`、`mesh.glb`、`metrics.json` を出力する。全身が画面高の68%未満、腕幅が画面幅の32%未満、中央ずれが12%超ならA/Tポーズ不適合として生成前に停止する。生成中はHugging Faceをオフライン固定し、初回セットアップ以外の外向き通信を許可しない。

`pipeline-probe` も実アプリと同じ設定優先順位を使い、`LVS_PIPELINE_MESH_RESOLUTION` などの環境変数を反映する。工程を再実行すると、その工程以降の状態と依存成果物を無効化する。古いリグ・中立キャプチャ・表情・投影を新しい上流成果物へ混在させない。

TripoSR経路はVRAM・工程接続の技術検証用であり、完成キャラクター用としては品質不適合である。現時点では3D化後の実画面を合格扱いせず、[TASKS.md](TASKS.md) T9の方式決定まで配布品質を主張しない。

`rig` はUVテクスチャ付き単一GLBを読み、正面A/Tポーズでない入力を明示エラーにする。AポーズはVRMのTポーズへ正規化し、自前のグラフheat diffusionで各頂点の上位4ウェイトを決める。出力は `rigged.vrm` と `rig-metrics.json`。同じPython 3.12環境のNumPy・SciPy・trimeshだけを使い、Blenderや追加モデルは同梱しない。

T3/T5 の実表示確認には vendored Three.js 0.185.1（MIT）を使う。`tools/facepatch-view/` をリポジトリルートからローカルHTTP配信し、`model` と、外部テクスチャを確認する場合だけ `texture` のクエリへローカルパスを渡す。投影テクスチャはアンリットで、VRMのglTF UV規約に合わせて外部PNGも `flipY=false` とする。`true` にするとUVアイランドが上下反転し、全身へ別部位が貼られる。製品の描画実装も `ui/shared/vendor/three/` を共有し、CDNへ接続しない。追加モジュール内の `three` 参照も同梱ファイルへの相対参照へ固定し、キャンバスの寸法変更は `ResizeObserver` で投影行列へ反映する。

固定環境は Python 3.12.13、PyTorch 2.11.0+cu128、torchvision 0.26.0+cu128、torchaudio 2.11.0+cu128、Transformers 4.57.6、xatlas 0.0.11。`sidecar/requirements-comfy.lock` はWindows x64向け全推移依存を版とwheelハッシュで固定している。依存を変える場合は `requirements-comfy.in` からlockfileを再生成し、同じGPU実走までやり直す。

`cargo xtask build` は Windows NSIS インストーラを `target/release/bundle/nsis/` へ出力する。署名と自動更新は販売方針決定後の後続タスクとする。

推論エンジン本体とモデルファイルはリポジトリに含めない（`.gitignore` 済み）。初回セットアップとモデル取得は時間がかかり、ネットワークが必要。

## 検証のコスト順序

[AGENTS.md](../AGENTS.md) の通り、安い順に試す。

1. `cargo check` — コンパイル
2. `cargo test --lib` — ロジック
3. `cargo xtask verify` — 目的別PRの最終ローカル検証をまとめるとき
4. **GUI 起動・生成の実走・スクリーンショットは高コストなので最後の手段。** 目視確認が本当に必要なときだけ、複数の確認をまとめて1回で行う

生成の実走（画像→3D、表情生成）は数分かかり GPU を占有する。パラメータを変えるたびに回さず、**変更が出揃ってから最小回数**だけ実行する。

## プロセスの後始末

保存保護の検証は `sidecar/.venv/Scripts/python.exe -m unittest discover -s sidecar -p test_output_transaction.py`、口の2軸輪郭は作業用Nodeで `node --test tools/test-mouth-geometry.mjs` を使う。いずれもモデル推論不要で、生成例外・公開失敗・中断復旧・二重実行拒否、および全パラメータ範囲の輪郭を検査する。Nodeは製品の起動・ビルド依存にはしない。

- 起動して確認したら、使い終わったプロセスは止める。ロックや再ビルドの無駄を避ける
- **子プロセス（Python サイドカー、推論エンジン）は `kill` して `wait` でハンドルまで回収する。** ゾンビになりやすい
- **プロセスを止める前に、必ず親プロセスとコマンドラインで「自分が起動したもの」かを確かめる。** `python.exe` や `node.exe` は他のアプリも使っているため、名前だけで一括終了すると稼働中のものを壊す

Windows での確認例:

```powershell
pwsh -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"Name='python.exe'\" | Select-Object ProcessId,ParentProcessId,CommandLine"
```

## 依存の更新

- **外部依存は原則として公式 LTS または長期保守安定版**を採用し、無ければ安定版 latest。推論エンジンとモデルは公式の最新安定版へ追従する
- **CUDA を使う Python ランタイムは Python 3.12 に統一する。** GPU・CUDA・PyTorch・CUDA拡張の公式 wheel が同時に対応する組み合わせを選ぶ
- `Cargo.toml` / `Cargo.lock` を変更したら `cargo audit` で既知脆弱性を確認する
- **モデル重みの商用利用条項・地域制限・再配布可否を採用前に確認し、結論を [SPEC.md](../SPEC.md) へ書く**（[AGENTS.md](../AGENTS.md)）

## 配布

Windows 先行で、現時点の配布形式は NSIS。署名・自動更新の要否は販売方針が決まってから判断する（[TASKS.md](TASKS.md) の「後続」）。

配布容量は十進の MB / GB で書く。バイト数の単独表示や併記をしない。
