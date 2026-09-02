# DEVELOPMENT.md — 開発・ビルド・配布

Rust と Tauri CLI だけで開発起動・テスト・Windows配布ビルドを行う。Node.js は不要。

## 必要なもの

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
sidecar/     Python 3.12（画像→3D、リギング）
docs/        ドキュメント
xtask/       開発タスク
temp/        一時作成物のみ。.gitignore 済み
```

## コマンド

| コマンド | 用途 |
|---|---|
| `cargo xtask setup engines` | llama.cpp / whisper.cpp のバイナリを取得 |
| `cargo xtask setup comfy` | 同梱 ComfyUI 本体・ワークフローと、監査済みの場合だけカスタムノード固定版を用意 |
| `cargo xtask setup sidecar` | Python 3.12 ランタイムと依存を用意（**CUDA wheel は対応GPU検出時のみ**） |
| `cargo xtask setup models` | モデルを取得 |
| `cargo xtask dev` | 開発起動 |
| `cargo xtask build` | 配布ビルド |
| `cargo xtask verify` | 書式・静的解析・テスト・文書同期をまとめて実行 |
| `cargo xtask facepatch --model <vrm/glb> --neutral <png> --expression-dir <36枚のディレクトリ> --atlas <png> --frame <json> --output-dir <dir> --diagnostics <dir>` | 中立スキニング済みメッシュへ36表情を逆投影 |
| `cargo xtask mesh --input <png> --output <dir>` | anime-segで背景除去し、TripoSRで2048² UVアトラス付きGLBを単発生成 |
| `cargo run -p local-vtuber-studio --bin lipsync-probe` | 既定マイクを3秒だけ16kHzへ変換し、FFT判定窓を検査して停止 |
| `cargo run -p local-vtuber-studio --bin stream-probe` | 透過OBSページを58090〜58099の空きポートで30秒配信し、女性3体の状態を切替 |

T2 の表情生成を試す場合は、次の順で一度だけセットアップする。

```powershell
cargo xtask setup comfy
cargo xtask setup sidecar
cargo xtask setup models
cargo xtask expression --input temp/input.png --output temp/expressions --identity-tags "髪・瞳・衣装・アクセサリの英語タグ"
```

`expression` の入力は 1024x1024 RGBA、出力は6表情×6口形の36 PNGと `metrics.json`。ComfyUIは `127.0.0.1:58120` のみで起動し、処理後は必ず終了してハンドルを回収する。初回起動の実測が180秒を超えたため、起動待ちは600秒とする。画像生成は denoise 0.65、閉眼の基準生成だけ0.85を用いる。顔全体ではなく左右の目と口の限定マスクを使い、最後に元画像へマスク合成するため、マスク外は画素単位で不変になる。

`setup sidecar` は `nvidia-smi` でCUDA対応GPUを確認してから、Python 3.12.13とハッシュ固定済み依存を単一環境へ同期する。`setup models` はT0で固定したリビジョンから取得し、SHA-256不一致なら採用せず中間ファイルを削除する。CPUフォールバックはない。

`mesh` は入力原本を `source/input.png` に保存し、`foreground.png`、`reconstruction-input.png`、`texture.png`、`mesh.glb`、`metrics.json` を出力する。全身が画面高の68%未満、腕幅が画面幅の32%未満、中央ずれが12%超ならA/Tポーズ不適合として生成前に停止する。生成中はHugging Faceをオフライン固定し、初回セットアップ以外の外向き通信を許可しない。

T3/T5 の実表示確認には vendored Three.js 0.185.1（MIT）を使う。`tools/facepatch-view/` をリポジトリルートからローカルHTTP配信し、`model` と、外部テクスチャを確認する場合だけ `texture` のクエリへローカルパスを渡す。投影テクスチャはアンリットで、PNGの上下方向をThree.jsのUVへ合わせるため `flipY=true` とする。製品の描画実装も `ui/shared/vendor/three/` を共有し、CDNへ接続しない。

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

- 起動して確認したら、使い終わったプロセスは止める。ロックや再ビルドの無駄を避ける
- **子プロセス（Python サイドカー、推論エンジン）は `kill` して `wait` でハンドルまで回収する。** ゾンビになりやすい
- **プロセスを止める前に、必ず親プロセスとコマンドラインで「自分が起動したもの」かを確かめる。** `python.exe` や `node.exe` は他のアプリも使っているため、名前だけで一括終了すると稼働中のものを壊す

Windows での確認例:

```bash
powershell -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"Name='python.exe'\" | Select-Object ProcessId,ParentProcessId,CommandLine"
```

## 依存の更新

- **外部依存は原則として公式 LTS または長期保守安定版**を採用し、無ければ安定版 latest。推論エンジンとモデルは公式の最新安定版へ追従する
- **CUDA を使う Python ランタイムは Python 3.12 に統一する。** GPU・CUDA・PyTorch・CUDA拡張の公式 wheel が同時に対応する組み合わせを選ぶ
- `Cargo.toml` / `Cargo.lock` を変更したら `cargo audit` で既知脆弱性を確認する
- **モデル重みの商用利用条項・地域制限・再配布可否を採用前に確認し、結論を [SPEC.md](../SPEC.md) へ書く**（[AGENTS.md](../AGENTS.md)）

## 配布

Windows 先行で、現時点の配布形式は NSIS。署名・自動更新の要否は販売方針が決まってから判断する（[TASKS.md](TASKS.md) の「後続」）。

配布容量は十進の MB / GB で書く。バイト数の単独表示や併記をしない。
