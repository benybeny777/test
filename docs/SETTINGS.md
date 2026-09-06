# SETTINGS.md — 全設定キー（正本）

このファイルが**設定キー一覧の唯一の正本**。実装（`src-tauri/src/config.rs`）との双方向同期テストが、記載漏れと削除漏れの両方を検出する。手作業の突き合わせは不要。

- 設定の保存・反映機構は [SPEC.md](../SPEC.md) 6 を参照
- 利用者がよく使う項目だけの説明は [README.md](../README.md) にあり、ここの全キーを複製しない
- 優先順位は **永続ファイル（`app_config_dir/config.json`） > 環境変数 > ハードコード既定値**
- `ai` の部分指定は指定キーだけを上書きし、省略キーの環境変数を保持する。例えばSAMのバッチ数だけを保存しても、環境変数で指定した安定性閾値を既定値へ戻さない。

実装済みキーは下表にまとめ、`config.rs` の双方向同期テストで記載漏れ・削除漏れを検出する。後半の「予定設定」は設計予約であり、実装済みキーには含めない。新しい設定を足したら**必ず同じ作業内でこの表を更新する**（[AGENTS.md](../AGENTS.md)）。

## 実装済み設定

<!-- implemented-settings:start -->
| キー | 型 | 既定値 | 環境変数 | 検証範囲 | 実行中反映 |
|---|---|---|---|---|---|
| `ai.blink_denoise` | f32 | 0.85 | `LVS_AI_BLINK_DENOISE` | 0〜1 | 次回生成 |
| `ai.completion_model_dir` | path | `qwen-eval` | `LVS_AI_COMPLETION_MODEL_DIR` | 空文字不可。相対指定は `ai.models_dir` 基準。承認済みQwen-Image-Edit-2511の重み配置先 | 次回局所補完 |
| `ai.completion_steps` | u32 | 50 | `LVS_AI_COMPLETION_STEPS` | 1〜100。局所補完の推論ステップ数 | 次回局所補完 |
| `ai.completion_seed` | u32 | 777 | `LVS_AI_COMPLETION_SEED` | 0〜4294967295。局所補完の再現用シード | 次回局所補完 |
| `ai.completion_resolution` | u32 | 1024 | `LVS_AI_COMPLETION_RESOLUTION` | 256〜4096の16倍数。原寸ROIの上限。引き伸ばし拡大はしない | 次回局所補完 |
| `ai.completion_mask_margin` | f32 | 0.2 | `LVS_AI_COMPLETION_MASK_MARGIN` | 0超〜0.5。検出領域の幅・高さに対する編集マスク余白の倍率 | 次回局所補完 |
| `ai.completion_mask_core_ratio` | f32 | 0 | `LVS_AI_COMPLETION_MASK_CORE_RATIO` | 0以上1未満。既存の目編集余白のうち強度255にする内側割合。`completion_mask_margin`と異なり余白の外縁は変えない。0は現在の条件を保持 | 保存後の次回局所補完 |
| `ai.completion_timeout_seconds` | u32 | 14400 | `LVS_AI_COMPLETION_TIMEOUT_SECONDS` | 1以上。局所補完の推論タイムアウト（秒） | 次回局所補完 |
| `ai.completion_fast_disk` | bool | true | `LVS_AI_COMPLETION_FAST_DISK` | 高速ディスクへのモデル退避を利用。処理速度とメモリ使用量に影響 | 次回局所補完 |
| `ai.completion_hidden_prompt` | string | 画風と既存形状の保持を指定する既定指示 | `LVS_AI_COMPLETION_HIDDEN_PROMPT` | 元の画風を維持して髪の下を補完する指示。空文字不可 | 次回局所補完 |
| `ai.completion_side_prompt` | string | 画風と既存形状の保持を指定する既定指示 | `LVS_AI_COMPLETION_SIDE_PROMPT` | 顔の位置と画風を維持して横髪の下の耳を補完する指示。空文字不可 | 次回局所補完 |
| `ai.completion_hidden_band_ratio` | f32 | 0.08 | `LVS_AI_COMPLETION_HIDDEN_BAND_RATIO` | 0超〜0.15。顔幅に対する隠れ顔の補完帯 | 次回局所補完 |
| `ai.completion_hidden_motion_ratio` | f32 | 0.35 | `LVS_AI_COMPLETION_HIDDEN_MOTION_RATIO` | 0超〜0.4。補完帯内の髪の移動倍率 | 次回局所補完 |
| `ai.completion_hair_edge_band_ratio` | f32 | 0.015 | `LVS_AI_COMPLETION_HAIR_EDGE_BAND_RATIO` | 0超〜0.05。顔幅に対する髪境界の再分類帯 | 次回局所補完 |
| `ai.completion_hair_edge_gain` | f32 | 40.0 | `LVS_AI_COMPLETION_HAIR_EDGE_GAIN` | 0超〜255。横髪補完との明度差による境界候補の条件 | 次回局所補完 |
| `ai.completion_ear_context` | f32 | 0.5 | `LVS_AI_COMPLETION_EAR_CONTEXT` | 0超〜2。耳候補の局所SAM解析の余白倍率 | 次回局所補完 |
| `ai.image_denoise` | f32 | 0.65 | `LVS_AI_IMAGE_DENOISE` | 0〜1 | 次回生成 |
| `ai.llm_model` | string | `qwen2.5-1.5b-instruct-q4_k_m.gguf` | `LVS_AI_LLM_MODEL` | 空文字不可 | 次回会話 |
| `ai.mesh_model` | string | `triposr` | `LVS_AI_MESH_MODEL` | 空文字不可 | 次回生成 |
| `ai.sam2_model` | string | `sam2.1-hiera-tiny` | `LVS_AI_SAM2_MODEL` | 空文字不可 | 次回レイヤー分解 |
| `ai.grounding_model` | string | `grounding-dino-base` | `LVS_AI_GROUNDING_MODEL` | 空文字不可。models_dir内のモデル配置 | 次回レイヤー分解 |
| `ai.grounding_threshold` | f32 | 0.20 | `LVS_AI_GROUNDING_THRESHOLD` | 0〜1。領域・語句の検出閾値 | 次回レイヤー分解 |
| `ai.eye_context_margin` | f32 | 0.5 | `LVS_AI_EYE_CONTEXT_MARGIN` | 0.1〜2。目の検出矩形の各辺へ足す幅/高さの倍率。最終素材は原寸 | 次回レイヤー分解。解析キャッシュ失効 |
| `ai.sam2_points_per_batch` | u32 | 8 | `LVS_AI_SAM2_POINTS_PER_BATCH` | 1〜64。探索点総数・原画解像度は変えない | 次回レイヤー分解 |
| `ai.sam2_pred_iou_threshold` | f32 | 0.7 | `LVS_AI_SAM2_PRED_IOU_THRESHOLD` | 0〜1。候補生成時の予測IoU閾値 | 次回レイヤー分解 |
| `ai.sam2_stability_threshold` | f32 | 0.85 | `LVS_AI_SAM2_STABILITY_THRESHOLD` | 0〜1。候補生成時の安定性閾値 | 次回レイヤー分解 |
| `ai.models_dir` | path | `models` | `LVS_AI_MODELS_DIR` | 相対または絶対 | 次回実行 |
| `ai.stt_model` | string | `ggml-small.bin` | `LVS_AI_STT_MODEL` | 空文字不可 | 次回認識 |
| `avatar.blink_duration_ms` | u32 | 140 | `LVS_AVATAR_BLINK_DURATION_MS` | 50〜2000 | 即時 |
| `avatar.blink_max_ms` | u32 | 6500 | `LVS_AVATAR_BLINK_MAX_MS` | 最短以上 | 即時 |
| `avatar.blink_min_ms` | u32 | 2800 | `LVS_AVATAR_BLINK_MIN_MS` | 500以上 | 即時 |
| `avatar.crossfade_ms` | u32 | 160 | `LVS_AVATAR_CROSSFADE_MS` | 0〜5000 | 即時 |
| `avatar.idle_sway_degrees` | f32 | 0.7 | `LVS_AVATAR_IDLE_SWAY_DEGREES` | 0〜10 | 即時 |
| `avatar.idle_sway_period_ms` | u32 | 4200 | `LVS_AVATAR_IDLE_SWAY_PERIOD_MS` | 500〜60000 | 即時 |
| `comfy.port` | u16 | 58120 | `LVS_COMFY_PORT` | 1〜65535、8188以外 | 次回生成 |
| `comfy.startup_timeout_seconds` | u32 | 600 | `LVS_COMFY_STARTUP_TIMEOUT_SECONDS` | 1以上 | 次回生成 |
| `comfy.unload_before_mesh` | bool | true | `LVS_COMFY_UNLOAD_BEFORE_MESH` | true固定 | 次回生成 |
| `comfy.workflow_dir` | path | `workflows` | `LVS_COMFY_WORKFLOW_DIR` | 相対または絶対 | 次回生成 |
| `display.language` | string | `ja` | `LVS_DISPLAY_LANGUAGE` | 空文字不可 | 次回起動 |
| `display.snapshot_record_bytes` | u32 | 0.096 MB | `LVS_DISPLAY_SNAPSHOT_RECORD_BYTES` | 正数。1素材≤合計、base64チャンク＋ヘッダ≤JSONレコード。超過はエラー（縮小しない） | 保存後の次回キャラ読込 |
| `display.snapshot_chunk_bytes` | u32 | 0.048 MB | `LVS_DISPLAY_SNAPSHOT_CHUNK_BYTES` | 正数。1素材≤合計、base64チャンク＋ヘッダ≤JSONレコード。超過はエラー（縮小しない） | 保存後の次回キャラ読込 |
| `display.snapshot_part_bytes` | u32 | 32 MB | `LVS_DISPLAY_SNAPSHOT_PART_BYTES` | 正数。1素材≤合計、base64チャンク＋ヘッダ≤JSONレコード。超過はエラー（縮小しない） | 保存後の次回キャラ読込 |
| `display.snapshot_total_bytes` | u32 | 256 MB | `LVS_DISPLAY_SNAPSHOT_TOTAL_BYTES` | 正数。1素材≤合計、base64チャンク＋ヘッダ≤JSONレコード。超過はエラー（縮小しない） | 保存後の次回キャラ読込 |
| `display.snapshot_parts` | u32 | 256 | `LVS_DISPLAY_SNAPSHOT_PARTS` | 正数。1素材≤合計、base64チャンク＋ヘッダ≤JSONレコード。超過はエラー（縮小しない） | 保存後の次回キャラ読込 |
| `display.snapshot_dimension` | u32 | 8192 | `LVS_DISPLAY_SNAPSHOT_DIMENSION` | 正数。1素材≤合計、base64チャンク＋ヘッダ≤JSONレコード。超過はエラー（縮小しない） | 保存後の次回キャラ読込 |
| `display.preview_fps` | u32 | 30 | `LVS_DISPLAY_PREVIEW_FPS` | 1〜240 | 次回起動 |
| `display.preview_scale` | f32 | 0.5 | `LVS_DISPLAY_PREVIEW_SCALE` | 0.1〜2.0 | 次回起動 |
| `facepatch.alpha_threshold` | f32 | 0.030 | — | 0〜1 | 次回投影 |
| `facepatch.depth_window` | f32 | 0.050 | — | 正数 | 次回投影 |
| `facepatch.diff_threshold` | f32 | 0.075 | — | 0〜1 | 次回投影 |
| `facepatch.face_mask_center` | [f32;2] | [0.500, 0.580] | — | 正規化座標 | 次回投影 |
| `facepatch.face_mask_radius` | [f32;2] | [0.240, 0.260] | — | 正数 | 次回投影 |
| `facepatch.head_weight_threshold` | f32 | 0.500 | — | 0〜1 | 次回投影 |
| `facepatch.max_ray_hits` | usize | 3 | — | 1以上 | 次回投影 |
| `facepatch.normal_threshold` | f32 | 0.350 | — | -1〜1 | 次回投影 |
| `facepatch.pipeline_version` | u32 | 2 | — | 1以上 | 次回投影 |
| `facepatch.seam_padding` | u32 | 4 | — | 0〜64 | 次回投影 |
| `import.alignment_tolerance` | f32 | 12.0 | `LVS_IMPORT_ALIGNMENT_TOLERANCE` | 0以上 | 次回取込 |
| `import.check_alignment` | bool | true | `LVS_IMPORT_CHECK_ALIGNMENT` | true / false | 次回取込 |
| `import.check_mirrored` | bool | true | `LVS_IMPORT_CHECK_MIRRORED` | true / false | 次回取込 |
| `import.color_tolerance` | f32 | 0.08 | `LVS_IMPORT_COLOR_TOLERANCE` | 0〜1 | 次回取込 |
| `lipsync.a_shape_bias` | f32 | 0.9 | `LVS_LIPSYNC_A_SHAPE_BIAS` | 0.1〜2.0 | 即時 |
| `lipsync.device_name` | string | `""` | `LVS_LIPSYNC_DEVICE_NAME` | 空なら既定マイク | 再接続時 |
| `lipsync.formants` | [table;5] | A/I/U/E/O | — | F1/F2が正数 | 即時 |
| `lipsync.interval_seconds` | f32 | 0.08 | `LVS_LIPSYNC_INTERVAL_SECONDS` | 0.01〜1.0 | 再接続時 |
| `lipsync.sample_rate` | u32 | 16000 | `LVS_LIPSYNC_SAMPLE_RATE` | 8000以上 | 再接続時 |
| `lipsync.silence_hold_seconds` | f32 | 0.16 | `LVS_LIPSYNC_SILENCE_HOLD_SECONDS` | 0.01〜2.0 | 即時 |
| `lipsync.smoothing_frames` | u32 | 4 | `LVS_LIPSYNC_SMOOTHING_FRAMES` | 1〜30 | 即時 |
| `lipsync.volume_gate_db` | f32 | -40.0 | `LVS_LIPSYNC_VOLUME_GATE_DB` | -100〜0 | 即時 |
| `lipsync.window_samples` | u32 | 512 | `LVS_LIPSYNC_WINDOW_SAMPLES` | 64以上の2の累乗 | 再接続時 |
| `pipeline.atlas_resolution` | u32 | 2048 | `LVS_PIPELINE_ATLAS_RESOLUTION` | 64〜8192 | 次回生成 |
| `pipeline.capture_resolution` | u32 | 1024 | `LVS_PIPELINE_CAPTURE_RESOLUTION` | 64〜4096 | 次回生成 |
| `pipeline.keep_intermediates` | bool | true | `LVS_PIPELINE_KEEP_INTERMEDIATES` | true固定 | 次回生成 |
| `pipeline.mesh_resolution` | u32 | 192 | `LVS_PIPELINE_MESH_RESOLUTION` | 32〜512 | 次回生成 |
| `pipeline.output_dir` | path | 空（`app_data_dir/characters`） | `LVS_PIPELINE_OUTPUT_DIR` | 相対または絶対 | 次回起動 |
| `vad.enabled` | bool | true | `LVS_VAD_ENABLED` | true / false | 即時 |
| `vad.end_silence_seconds` | f32 | 0.9 | `LVS_VAD_END_SILENCE_SECONDS` | 正数 | 即時 |
| `vad.max_seconds` | f32 | 6.0 | `LVS_VAD_MAX_SECONDS` | 最短以上 | 即時 |
| `vad.min_seconds` | f32 | 0.5 | `LVS_VAD_MIN_SECONDS` | 正数 | 即時 |
<!-- implemented-settings:end -->

保存先は `app_config_dir/config.json`。優先順位は永続ファイル、環境変数、既定値の順。未知キーや不正値を含むファイルは `.corrupt` へ退避し、標準エラーへ理由を出して既定値で起動する。

## 予定設定

中立画像への幾何位置合わせと色補正は未実装で、`facepatch.align_to_neutral` と `facepatch.color_match` はまだ設定として公開しない。現行の外部画像取り込みは位置・反転・色差を検査して警告するところまでとする。

`gpu_backend` の設定は持たない。CUDA 必須が決定事項であり、CPU フォールバックを実装しないため選択肢が存在しない。

## 命名規則

- 環境変数名は `LVS_` を接頭辞とする
- 秘密値を持つキーは作らない。本実装はクラウドAPIを使わないため。将来必要になった場合の規則は [AGENTS.md](../AGENTS.md) の「設定値・シークレット」を参照
