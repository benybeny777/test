---
name: add-picovtuber-setting
description: PicoVTuberへ設定項目を追加・変更・削除し、永続化、設定画面、実行中反映、秘密値保護、設定キー正本、利用文書、双方向ガードまで同期する依頼で使う。
---

# PicoVTuberの設定を変更する

## 調査

1. `AGENTS.md`の「設定値・シークレット」を読む。
2. 設定キー、`config_schema()`、`ui/settings.js`、`settings_*` command、`docs/SETTINGS.md`、README、MANUAL、SPECを`rg`で検索する。
3. 新しい画面や保存経路を増やす前に、既存タブ・既存スキーマ・既存commandを拡張できるか確認する。

## 実装

1. 正本を`src-tauri/src/config.rs`の永続設定に置き、環境変数は既定値としてだけ使う。キーは`PICOVTUBER_`接頭辞。
2. コネクタ設定は`config_schema()`へ申告し、値を保持せず利用時に`cfg.get()`で読む。
3. 秘密値を持つ場合は`type="password"`と秘密値として判定できるキー名（`API_KEY`／`TOKEN`／`SECRET`／`PASSWORD`／`WEBHOOK`／`CREDENTIAL`のいずれかを含む）を使う。設定画面が直接作る項目も同じ暗号化経路へ乗ることを確認する。
4. 保存後に再起動なしで反映する。一覧JSONや状態ファイルを更新する場合は`AGENTS.md`のロック、破損退避、原子的書き込み規則も適用する。
5. キーの追加・変更・削除を`docs/SETTINGS.md`へ同じ作業で反映する。利用者が日常的に触るキーならREADMEとMANUAL、保存・反映方式を変えた場合はSPECも更新し、変更理由と検証結果をPR本文へ書く。
6. 設定キーではない`PICOVTUBER_*`文字列（テスト用の識別子など）を足す場合は、理由付きで`doc_sync`の`NOT_CONFIG_KEYS`へ登録する。

## 検証

1. 設定画面で初期値、変更、保存、再読込、未保存表示、実行中反映を確認する。
2. 秘密値は画面、`config.json`、ログ、エラーへ平文で出ないことを確認する。
3. `doc_sync::guard`、`secret::guard`、関係する単体テスト、`cargo check`、JS構文、`git diff --check`を通す。
4. 実画面の配置や操作が変わる場合は`verify-picovtuber-studio`も使う。
