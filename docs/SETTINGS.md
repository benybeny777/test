# docs/SETTINGS.md — 全設定キーの一覧（正本）

このファイルが設定キーの唯一の一覧です。**実装とこの一覧のズレは `doc_sync::guard` のテストが双方向で検出します**（載せ忘れも、削除漏れも落ちます）。手作業で突き合わせる必要はありません。

- 値の優先順位: **永続ファイル（`app_config_dir/config.json`）> 環境変数 > ハードコード既定値**
- 設定画面（GUI）が唯一の編集入口です。保存後は再起動なしで反映されます（各所が `cfg.get()` で都度読むため）。
- 環境変数は**既定値としてだけ**参照します。保存先にはしません。
- キーを追加・変更・削除するときは `.agents/skills/add-picovtuber-setting/SKILL.md` の手順に従い、この一覧を同じ作業で更新してください。

## ファイルの許可

| キー | 既定 | 説明 |
|---|---|---|
| `PICOVTUBER_ALLOWED_ROOT` | （なし） | イラストの読み込みと成果物の書き出しを、このフォルダの中だけに限る。**未設定だと生成は始まりません** |
| `PICOVTUBER_ALLOWED_PATHS` | （なし） | 追加で許可するパス。OS のパス区切り文字で複数指定 |
| `PICOVTUBER_DENIED_PATHS` | （なし） | 拒否するパス。**許可より先に効きます**（許可フォルダの中に例外を置けます） |

`C:\Windows` / `Program Files`（Windows）、`/bin` `/sbin` `/usr/bin` `/etc`（その他）は、設定に関わらず常に拒否されます。

## ランタイムと生成モデル

| キー | 既定 | 説明 |
|---|---|---|
| `PICOVTUBER_RUNTIME_PYTHON` | 管理下の Python | 工程スクリプトを動かす Python 実行ファイル |
| `PICOVTUBER_RUNTIME_DEVICE` | `auto` | 実行デバイス。`auto` / `cpu` / `cuda`。未知の値は `auto` へ落ちます |
| `PICOVTUBER_RUNTIME_TIMEOUT_SEC` | `1800` | 工程スクリプト1回あたりの上限（秒）。超えたらプロセスツリーごと中断します |
| `PICOVTUBER_MODELS_DIR` | `<アプリデータ>/PicoVTuber/models` | 生成モデルの重みの置き場所 |
| `PICOVTUBER_SCRIPTS_DIR` | `scripts` | 工程スクリプトの置き場所 |

## 生成工程

| キー | 既定 | 工程 | 説明 |
|---|---|---|---|
| `PICOVTUBER_PREPROCESS_MAX_EDGE` | `2048` | 下ごしらえ | 正規化後の最大辺。**これを超える絵だけ縮めます**（引き伸ばしはしません） |
| `PICOVTUBER_PREPROCESS_MIN_EDGE` | `1024` | 下ごしらえ | 必要な最小の長辺。下回る絵は理由を出して止まります |
| `PICOVTUBER_PREPROCESS_TRIM_ALPHA` | `8` | 下ごしらえ | 余白と見なすアルファ値（0〜255） |
| `PICOVTUBER_SEGMENT_MODEL` | `segment.onnx` | パーツ分割 | 重みのファイル名 |
| `PICOVTUBER_MULTIVIEW_MODEL` | `multiview.onnx` | 多視点生成 | 重みのファイル名 |
| `PICOVTUBER_MULTIVIEW_STEPS` | `30` | 多視点生成 | 生成ステップ数。多いほど安定しますが時間が伸びます |
| `PICOVTUBER_MESH_MODEL` | `mesh.onnx` | メッシュ化 | 重みのファイル名 |
| `PICOVTUBER_MESH_TARGET_FACES` | `30000` | メッシュ化 | 目標ポリゴン数 |
| `PICOVTUBER_RIG_HEIGHT_M` | `1.6` | ボーン設定 | モデルの身長（メートル） |
| `PICOVTUBER_EXPRESSION_STRENGTH` | `1.0` | 表情生成 | 表情の強さ（0.2〜2.0） |
| `PICOVTUBER_VISEME_STRENGTH` | `1.0` | 口形生成 | 口の開きの強さ（0.2〜2.0） |
| `PICOVTUBER_PACK_TITLE` | （なし） | 書き出し | VRM メタデータのモデル名 |
| `PICOVTUBER_PACK_AUTHOR` | （なし） | 書き出し | VRM メタデータの作者名 |
| `PICOVTUBER_PACK_LICENSE_NOTICE` | （なし） | 書き出し | 利用許諾。**空のままでは書き出せません** |

## 生成ジョブ

| キー | 既定 | 説明 |
|---|---|---|
| `PICOVTUBER_JOB_MAX_AUTO_RETRY` | `5` | 自動での再試行の上限。利用者が明示した再試行では数え直します |

## 配信に使うモデル

| キー | 既定 | 説明 |
|---|---|---|
| `PICOVTUBER_MODEL_PATH` | （なし） | 配信に使う VRM のパス。生成したモデルでも、手元の VRM 1.0 でも構いません |

## リップシンク

音声はこのPCの中だけで処理し、保存も送信もしません。調整の目安は [MANUAL.md](../MANUAL.md) の対応表を参照してください。

| キー | 既定 | 説明 |
|---|---|---|
| `PICOVTUBER_LIPSYNC_ENABLED` | `true` | マイク入力を解析して口を動かすか |
| `PICOVTUBER_LIPSYNC_INPUT_DEVICE` | （なし） | 入力デバイス。空なら OS の既定 |
| `PICOVTUBER_LIPSYNC_SILENCE_RMS` | `0.012` | これを下回る音量は無音とみなし、口を閉じます |
| `PICOVTUBER_LIPSYNC_GAIN` | `8.0` | 音量を口の開き量へ写す倍率 |
| `PICOVTUBER_LIPSYNC_SMOOTHING` | `0.35` | 平滑化の強さ。小さいほど滑らか、大きいほど追従が速い |
| `PICOVTUBER_LIPSYNC_FRAME_MS` | `20` | 1フレームの長さ（ミリ秒） |

## 表情

| キー | 既定 | 説明 |
|---|---|---|
| `PICOVTUBER_EXPRESSION_BLEND_MS` | `250` | 表情の切り替えにかける時間（ミリ秒）。0 は瞬間切替 |
| `PICOVTUBER_EXPRESSION_DEFAULT` | （なし） | 起動時の表情。`joy` / `angry` / `surprised` / `sorrow` / `fun`。未知の値は素の顔 |

## 表情の自動切替（ローカルAI）

**両方のモデルが揃っているときだけ有効になります。** 欠けている場合は「利用不可」と表示され、クラウドサービスでは代替しません。

| キー | 既定 | 説明 |
|---|---|---|
| `PICOVTUBER_AUTO_EXPRESSION_ENABLED` | `false` | 会話内容から表情を自動で切り替えるか |
| `PICOVTUBER_AUTO_EXPRESSION_INTERVAL_SEC` | `5` | 判定の間隔（秒）。短いと顔がちらつきます |
| `PICOVTUBER_AUTO_EXPRESSION_MIN_CONFIDENCE` | `0.34` | これを下回る確信度では切り替えません |
| `PICOVTUBER_ASR_MODEL_PATH` | （なし） | 音声認識モデルのパス |
| `PICOVTUBER_LLM_MODEL_PATH` | （なし） | 話題・感情を判定する小型LLMのパス |

## 配信出力

| キー | 既定 | 出力 | 説明 |
|---|---|---|---|
| `PICOVTUBER_OUTPUT_WINDOW_WIDTH` | `1280` | 透過ウィンドウ | 幅（160〜7680に収めます） |
| `PICOVTUBER_OUTPUT_WINDOW_HEIGHT` | `720` | 透過ウィンドウ | 高さ（160〜4320に収めます） |
| `PICOVTUBER_OUTPUT_WINDOW_ALWAYS_ON_TOP` | `false` | 透過ウィンドウ | 常に最前面に置くか |
| `PICOVTUBER_OUTPUT_FPS` | `30` | 仮想カメラ | 出力フレームレート |

## 秘密値の扱い

いまのところ秘密値を持つ設定キーはありません（クラウド推論を行わないため、外部サービスのAPIキーが要りません）。

将来、配信ソフト連携などで秘密値を持つ場合は、キー名に `API_KEY` / `TOKEN` / `SECRET` / `PASSWORD` / `WEBHOOK` / `CREDENTIAL` のいずれかを含め、`type="password"` で申告してください。`secret.rs` がOSの資格情報保護（Windows: DPAPI、macOS: ログインキーチェーン）へ預けます。Linux は同等のOS機能を使っていないため平文保存です。
