# 共通作業スキル

このフォルダは、Codex・Claude Codeなど複数の作業エージェントで共有する手順の正本です。

| スキル | 用途 |
|---|---|
| `add-picovtuber-connector` | 生成工程（Stage）／配信出力（Output）コネクタの追加・変更 |
| `add-picovtuber-setting` | 設定項目の追加・変更（永続化、画面、秘密値、文書） |
| `audit-picovtuber-dependencies` | パッケージ、同梱ランタイム、生成モデル重みの依存監査 |
| `generate-picovtuber-model` | 1枚絵からのモデル生成、工程の再実行、生成品質の検証 |
| `verify-picovtuber-studio` | 配信モードの実画面QA（透過、描画、リップシンク、表情、配信出力） |
| `prepare-picovtuber-release` | 配布前のコード、資産、文書、監査、ローカルテスト、Git確認 |

Claude Codeが自動認識する `.claude/skills/<名前>/SKILL.md` には、同名の共通スキル正本を読む指示だけを置きます。実際の手順を追加・修正するときは、このフォルダの正本だけを編集してください。名前一致、参照先、frontmatter、自動認識ファイル本文の肥大化は `cargo xtask verify` が検査します。
