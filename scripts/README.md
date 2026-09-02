# 工程スクリプト

生成パイプラインのうち、Python 側で行う工程の入口です。呼び出すのは
`src-tauri/src/pipeline/runtime.rs` で、**PicoVTuber 管理下の Python** から起動します。

- **外部の推論APIは呼びません。** 重みは利用者のPC内にあるものだけを使います。
- 進捗は標準出力へ `PROGRESS <0.0-1.0> <メッセージ>` の形で出します。それ以外の行は
  診断用としてまとめて拾われます。
- 失敗は**非ゼロの終了コード**と標準エラーの理由で返します。**白紙や複製で埋めて
  成功にしないでください**（欠損したまま最後まで通ると、利用者は配信本番で気づきます）。
- 引数の契約を変えるときは、同じPRで `pipeline/stages/` と `SPEC.md` も更新してください。

| スクリプト | 工程 | 主な引数 | 出力 |
|---|---|---|---|
| `segment.py` | パーツ分割 | `--input` `--output-dir` `--model` `--parts` `--device` | `<出力先>/<パーツ>.png` |
| `multiview.py` | 多視点生成 | `--input` `--masks-dir` `--output-dir` `--model` `--steps` `--views` `--device` | `<出力先>/<ビュー>.png` |
| `mesh.py` | メッシュ化 | `--views-dir` `--output-dir` `--model` `--target-faces` `--device` | `<出力先>/mesh.glb`, `texture.png` |
| `rig.py` | ボーン設定 | `--mesh` `--bones` `--output-dir` `--device` | `<出力先>/rig.glb` |
| `pack.py` | 書き出し | `--rig` `--expressions-dir` `--visemes-dir` `--texture` `--meta` `--output` `--device` | `model.vrm` |

## 現状

**推論本体と glTF 組み立ては未実装です。** 採用するモデルとライブラリを決めてから実装します
（着手条件は [docs/TASKS.md](../docs/TASKS.md)）。いまは引数の契約を確定させ、未実装で
あることを終了コードと理由で返します。**動いていないものを「動いた」と見せません。**
