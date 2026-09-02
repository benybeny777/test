# DEVELOPMENT.md — 開発・ビルド・配布

> **現在は設計段階です。** 下記のコマンドは T1（[TASKS.md](TASKS.md)）で実装します。実装した時点で「予定」の記述を消し、実際に動く手順へ更新してください。

## 必要なもの

| 項目 | 版・備考 |
|---|---|
| Rust（cargo） | 安定版 latest |
| Tauri CLI | `cargo install tauri-cli --version '^2'` |
| WebView2 ランタイム | Windows のみ。多くの環境で導入済み |
| CUDA 対応 NVIDIA GPU | 画像→3D と表情生成に必要。VRAM 8GB 以上を推奨 |

**Node.js はアプリとビルドの依存にしない。** 作業用ツール（スクリーンショット撮影など）としての利用は可。判断基準は「アプリのビルド・起動・配布に Node が要るようになるか」（[AGENTS.md](../AGENTS.md)）。

**Python は利用者側では不要。** 画像→3D とリギングの2工程だけが Python 3.12 を使い、ランタイムを同梱する（[SPEC.md](../SPEC.md) 3.1）。開発時は `cargo xtask setup sidecar` が用意する。

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

## コマンド（予定）

| コマンド | 用途 |
|---|---|
| `cargo xtask setup engines` | llama.cpp / whisper.cpp / stable-diffusion.cpp のバイナリを取得 |
| `cargo xtask setup sidecar` | Python 3.12 ランタイムと依存を用意（**CUDA wheel は対応GPU検出時のみ**） |
| `cargo xtask setup models` | モデルを取得 |
| `cargo xtask dev` | 開発起動 |
| `cargo xtask build` | 配布ビルド |
| `cargo xtask verify` | 書式・静的解析・テスト・文書同期をまとめて実行 |

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

## 配布（予定）

Windows 先行。バンドル形式・署名・自動更新の要否は販売方針が決まってから判断する（[TASKS.md](TASKS.md) の「後続」）。

配布容量は十進の MB / GB で書く。バイト数の単独表示や併記をしない。
