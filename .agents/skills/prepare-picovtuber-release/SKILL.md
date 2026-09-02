---
name: prepare-picovtuber-release
description: PicoVTuberの配布・リリース前に、実装、文書、第三者資産のライセンス、ローカルテスト、依存監査、配布設定、Git差分をまとめて検証する。リリースしてよいか、配布物を作る、mainをきれいにしてpushする依頼で使う。
---

# PicoVTuber配布前検証

## 最初に読む

1. `AGENTS.md`、`HANDOFF.md`、`THIRD_PARTY_NOTICES.md`を読む。
2. `git status --short`、現在ブランチ、直近コミット、`origin`との差を確認する。
3. 既存変更を利用者または別エージェントの作業として扱い、巻き戻さない。

## 検証する

1. `cargo check --manifest-path src-tauri/Cargo.toml`を行う。
2. `cargo test --lib --manifest-path src-tauri/Cargo.toml`と`cargo test --manifest-path xtask/Cargo.toml`を行う。設計ガードもここで走る。
3. `cargo xtask verify`を通す。
4. `Cargo.toml`または`Cargo.lock`を変更した場合だけ、対象lockfileへ`cargo audit --file <Cargo.lock>`を実行する。
5. **第三者資産の再配布が発生していないことを確認する。** 生成モデルの重み、ランタイム、three.js／three-vrmがリリース資産へ混ざっていないこと。`THIRD_PARTY_NOTICES.md`の表が現行の取得対象と一致すること。
6. **クラウド推論が混入していないことを確認する。** `cloud_free::guard`が通ること、`rg`で外部推論サービスのエンドポイントが無いこと。
7. `cargo xtask build`または依頼で指定された配布検査を行う。Ubuntu Linuxはdebの導入・起動・削除とAppImageの起動・音声依存を実成果物で確認する。
8. `git diff --check`、文書同期ガード、配布対象と公開資料の整合を確認する。

## 判定する

- 実行していない確認を合格と書かない。GPU・マイク・配信ソフトが要る確認は、この環境では未確認として分けて書く。
- 警告、保留、環境由来の未確認はPR本文へ理由と再確認方法を書き、リリース後も残る項目は`docs/TASKS.md`へ移す。
- 起動中アプリのファイルロックで通常ビルド先を使えない場合、利用者のアプリを勝手に止めず、`temp/`配下の隔離`--target-dir`で再検証する。
- 変更と検証が完了したら`AGENTS.md`のルールどおり全差分をコミットし、PRをsquash mergeして作業ブランチを削除する。利用者が「マージしない」「PRだけ」と明示した場合は従う。
