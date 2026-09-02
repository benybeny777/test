# SETTINGS.md — 全設定キー（正本）

このファイルが**設定キー一覧の唯一の正本**。実装（`src-tauri/src/config.rs`）との双方向同期テストが、記載漏れと削除漏れの両方を検出する。手作業の突き合わせは不要。

- 設定の保存・反映機構は [SPEC.md](../SPEC.md) 6 を参照
- 利用者がよく使う項目だけの説明は [README.md](../README.md) にあり、ここの全キーを複製しない
- 優先順位は **永続ファイル（`app_config_dir/config.json`） > 環境変数 > ハードコード既定値**

実装済みキーは下表にまとめ、`config.rs` の双方向同期テストで記載漏れ・削除漏れを検出する。後半の「予定設定」は設計予約であり、実装済みキーには含めない。新しい設定を足したら**必ず同じ作業内でこの表を更新する**（[AGENTS.md](../AGENTS.md)）。

## 実装済み設定

<!-- implemented-settings:start -->
| キー | 型 | 既定値 | 環境変数 | 検証範囲 | 実行中反映 |
|---|---|---|---|---|---|
| `avatar.blink_duration_ms` | u32 | 140 | `LVS_AVATAR_BLINK_DURATION_MS` | 50〜2000 | 即時 |
| `avatar.blink_max_ms` | u32 | 6500 | `LVS_AVATAR_BLINK_MAX_MS` | 最短以上 | 即時 |
| `avatar.blink_min_ms` | u32 | 2800 | `LVS_AVATAR_BLINK_MIN_MS` | 500以上 | 即時 |
| `avatar.crossfade_ms` | u32 | 160 | `LVS_AVATAR_CROSSFADE_MS` | 0〜5000 | 即時 |
| `avatar.idle_sway_degrees` | f32 | 0.7 | `LVS_AVATAR_IDLE_SWAY_DEGREES` | 0〜10 | 即時 |
| `avatar.idle_sway_period_ms` | u32 | 4200 | `LVS_AVATAR_IDLE_SWAY_PERIOD_MS` | 500〜60000 | 即時 |
| `display.language` | string | `ja` | `LVS_DISPLAY_LANGUAGE` | 空文字不可 | 次回起動 |
| `display.preview_fps` | u32 | 30 | `LVS_DISPLAY_PREVIEW_FPS` | 1〜240 | 次回起動 |
| `display.preview_scale` | f32 | 0.5 | `LVS_DISPLAY_PREVIEW_SCALE` | 0.1〜2.0 | 次回起動 |
| `lipsync.a_shape_bias` | f32 | 0.9 | `LVS_LIPSYNC_A_SHAPE_BIAS` | 0.1〜2.0 | 即時 |
| `lipsync.device_name` | string | `""` | `LVS_LIPSYNC_DEVICE_NAME` | 空なら既定マイク | 再接続時 |
| `lipsync.formants` | [table;5] | A/I/U/E/O | — | F1/F2が正数 | 即時 |
| `lipsync.interval_seconds` | f32 | 0.08 | `LVS_LIPSYNC_INTERVAL_SECONDS` | 0.01〜1.0 | 再接続時 |
| `lipsync.sample_rate` | u32 | 16000 | `LVS_LIPSYNC_SAMPLE_RATE` | 8000以上 | 再接続時 |
| `lipsync.silence_hold_seconds` | f32 | 0.16 | `LVS_LIPSYNC_SILENCE_HOLD_SECONDS` | 0.01〜2.0 | 即時 |
| `lipsync.smoothing_frames` | u32 | 4 | `LVS_LIPSYNC_SMOOTHING_FRAMES` | 1〜30 | 即時 |
| `lipsync.volume_gate_db` | f32 | -40.0 | `LVS_LIPSYNC_VOLUME_GATE_DB` | -100〜0 | 即時 |
| `lipsync.window_samples` | u32 | 512 | `LVS_LIPSYNC_WINDOW_SAMPLES` | 64以上の2の累乗 | 再接続時 |
| `obs.enabled` | bool | false | `LVS_OBS_ENABLED` | true / false | 即時 |
| `obs.port_range_end` | u16 | 58099 | `LVS_OBS_PORT_RANGE_END` | 開始以上 | 再起動時 |
| `obs.port_range_start` | u16 | 58090 | `LVS_OBS_PORT_RANGE_START` | 終了以下 | 再起動時 |
| `vad.enabled` | bool | true | `LVS_VAD_ENABLED` | true / false | 即時 |
| `vad.end_silence_seconds` | f32 | 0.9 | `LVS_VAD_END_SILENCE_SECONDS` | 正数 | 即時 |
| `vad.max_seconds` | f32 | 6.0 | `LVS_VAD_MAX_SECONDS` | 最短以上 | 即時 |
| `vad.min_seconds` | f32 | 0.5 | `LVS_VAD_MIN_SECONDS` | 正数 | 即時 |
<!-- implemented-settings:end -->

保存先は `app_config_dir/config.json`。優先順位は永続ファイル、環境変数、既定値の順。未知キーや不正値を含むファイルは `.corrupt` へ退避し、標準エラーへ理由を出して既定値で起動する。

## 予定設定

## 命名規則

- 環境変数名は `LVS_` を接頭辞とする（製品名確定時に一括変更する）
- 秘密値を持つキーは作らない。本実装はクラウドAPIを使わないため。将来必要になった場合の規則は [AGENTS.md](../AGENTS.md) の「設定値・シークレット」を参照

## 生成パイプライン

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `pipeline.capture_resolution` | u32 | 1024 | 顔キャプチャの解像度（正方形） |
| `pipeline.atlas_resolution` | u32 | 2048 | 投影先UVアトラスの解像度 |
| `pipeline.output_dir` | path | `app_data_dir/characters` | キャラクター保存先 |
| `pipeline.keep_intermediates` | bool | true | 各工程の中間成果物を残すか。途中再開に必要 |

## フェイスパッチ投影

パラメータの意味は [SPEC.md](../SPEC.md) 4.4 を参照。**すべて設定から都度読み、ソースへ散らさない**（[AGENTS.md](../AGENTS.md)）。

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `facepatch.pipeline_version` | u32 | 1 | 署名に含める版番号。**署名対象を増やしたら上げる** |
| `facepatch.diff_threshold` | f32 | 0.075 | 中立との差分がこれ以下の画素は書き込まない |
| `facepatch.alpha_threshold` | f32 | 0.030 | これ未満のαは対象外 |
| `facepatch.seam_padding` | u32 | 4 | UV継ぎ目の膨張画素数 |
| `facepatch.head_weight_threshold` | f32 | 0.500 | 三角形採用に必要な頭ボーンウェイト |
| `facepatch.normal_threshold` | f32 | 0.350 | 面法線がキャプチャ方向を向いている度合い |
| `facepatch.max_ray_hits` | u32 | 3 | レイキャストの最大ヒット数 |
| `facepatch.depth_window` | f32 | 0.050 | 最前面判定の深度窓 |
| `facepatch.face_mask_center` | [f32;2] | [0.500, 0.580] | 顔マスク楕円の中心（正規化座標） |
| `facepatch.face_mask_radius` | [f32;2] | [0.240, 0.260] | 顔マスク楕円の半径（正規化座標） |
| `facepatch.align_to_neutral` | bool | true | 投影前に中立へ位置合わせする |
| `facepatch.color_match` | bool | true | 投影前に色味を合わせる |

## ローカルAIエンジン

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `ai.models_dir` | path | `app_data_dir/models` | モデルの保存先 |
| `ai.llm_model` | string | 未定 | 会話・表情選択LLMの GGUF 名（1.5B級で足りる見込み） |
| `ai.stt_model` | string | 未定 | whisper.cpp のモデル名 |
| `ai.image_denoise` | f32 | 0.65 | 表情・口形インペイントの実測採用値（[SPEC.md](../SPEC.md) 4.7.1） |
| `ai.blink_denoise` | f32 | 0.85 | 閉眼の基準画像だけに使う実測採用値 |
| `ai.mesh_model` | string | 未定 | 画像→3D モデル名 |

**`gpu_backend` の設定は持たない。** CUDA 必須が決定事項であり、CPU フォールバックを実装しないため、選択肢が存在しない（[SPEC.md](../SPEC.md) 8）。

## ComfyUI（同梱）

[SPEC.md](../SPEC.md) 3.1 の「ComfyUI 固有の規則」を参照。**利用者の既存 ComfyUI 環境を読まない・書かない・ポートを奪わない。**

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `comfy.port` | u16 | 58120 | 同梱ComfyUIの待受ポート。既定8188を避ける |
| `comfy.startup_timeout_seconds` | u32 | 600 | 初回起動が180秒を超えた実測に基づく上限 |
| `comfy.workflow_dir` | path | 同梱 `workflows` | ワークフローJSONの置き場 |
| `comfy.unload_before_mesh` | bool | true | 画像→3D生成の前にComfyUIのモデルをアンロードしてVRAMを空ける（[SPEC.md](../SPEC.md) 3.1） |

## 外部生成画像のインポート（[SPEC.md](../SPEC.md) 4.10 D）

| キー | 型 | 既定値 | 説明 |
|---|---|---|---|
| `import.check_alignment` | bool | true | 投入画像の位置ずれを中立と照合して警告する |
| `import.check_mirrored` | bool | true | 左右反転を検出して警告する |
| `import.alignment_tolerance` | f32 | 未定 | 許容するずれ量。超えたら警告 |
