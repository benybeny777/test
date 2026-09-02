# HANDOFF.md — 未マージ作業の引き継ぎ

恒久ルールは [AGENTS.md](AGENTS.md)、未完了項目は [docs/TASKS.md](docs/TASKS.md)、過去の経緯はPR本文。
ここには「いま未マージで、次の担当がすぐ再開できるようにするための最小情報」だけを置く。

## 現在の未マージ作業

- ブランチ: `claude/twitter-link-implementation-hpvnw1`
- 内容: PicoVTuber の初期実装（ルール正本・共通層・生成パイプライン・配信機能・UI・xtask）。
- 再開方法: `git switch claude/twitter-link-implementation-hpvnw1` → `cargo check --manifest-path src-tauri/Cargo.toml`。
  Linux コンテナでは `.claude/hooks/session-start.sh` が先に必要な apt 依存と `ui-dist/` を用意する。
- 未確認: GPU・マイク・配信ソフトを要する検証（ML工程の実走、リップシンクの実音声、OBS取り込み）。
  条件と着手手順は [docs/TASKS.md](docs/TASKS.md) を参照。
