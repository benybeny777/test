# docs/TASKS.md — 未完了・条件待ちだけを置く一覧

完了した作業はPR本文へ残し、この一覧からは削除する（完了記録の二重管理をしない）。

## 実機・GPU・周辺機器が要るため、この環境で完了できない

| 項目 | 着手条件 | 内容 |
|---|---|---|
| ML工程の実走検証 | CUDA対応GPUのあるWindows/Linux実機、`cargo xtask setup models` 完了 | `segment` / `multiview` / `mesh` を実際の重みで通し、成果物の寸法・品質・所要時間を記録する。CPU版でどこまで実用かも同時に測る |
| リップシンクの実音声検証 | マイクのある実機 | 日本語の実発話で口形6種の当たり方を確認し、既定の閾値（`PICOVTUBER_LIPSYNC_SILENCE_RMS` / `GAIN` / `SMOOTHING`）を実測値へ寄せる |
| 配信出力の取り込み検証 | OBS を入れた実機 | 透過ウィンドウがOBSのウィンドウキャプチャで背景透過のまま取り込めること、仮想カメラが映像キャプチャデバイスに出ることをOSごとに確認する |
| three-vrm 描画の目視確認 | `cargo xtask setup viewer` 完了、GUI環境 | 生成したVRMを読み込み、表情・口形・ボーンが意図どおり動くことを確認する |

## 実装が未完了

| 項目 | 着手条件 | 内容 |
|---|---|---|
| ML工程の推論スクリプト本体 | 採用する重み（分割・多視点・メッシュ化）の決定とライセンス確認 | `scripts/` の各スクリプトは入出力の契約とCLI引数を確定させてあるが、推論本体は採用モデルが決まってから実装する。契約を変える場合は同じPRで `pipeline/stages/` と `SPEC.md` も更新する |
| 仮想カメラ出力 | 対象OSごとの仮想カメラ実装方式の決定 | `virtual_camera` は現在 `availability()` が理由付きで未対応を返す。Windows/macOS それぞれの方式（DirectShow フィルタ／CoreMediaIO DAL）を決めてから実装する |
| アプリ更新の配信 | 配布方針（署名・更新鍵・配信先）の決定 | 版番号管理（`cargo xtask bump-version`）までは用意済み。自動更新は署名方針を決めてから |
| ランタイムと重みの自動取得 | 採用する Python 配布形式・CUDA 組合せ・生成モデルとライセンスの確定 | `cargo xtask setup runtime` / `setup models` は現在、理由を出して止まる。決まり次第、取得URL・SHA-256・OS/GPU別のアセット選択を実装し、`THIRD_PARTY_NOTICES.md` へ出所を記載する |
| アプリアイコン | 1024px のアイコン原画 | いま `src-tauri/icons/` にあるのは暫定の生成画像。原画ができたら `npx tauri icon <原画>` で全形式（`.ico` / `.icns` を含む）を作り直し、`tauri.conf.json` の `icon` 一覧へ戻す |
