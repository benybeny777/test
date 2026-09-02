# THIRD_PARTY_NOTICES.md — 第三者資産の出所とライセンス

PicoVTuber は、生成モデルの重み・ランタイム・Webライブラリを**リリース資産へ再配布しません**。
利用者のPCが公式配布元から直接取得します（`cargo xtask setup`）。それぞれのライセンスは配布元の条件に従います。

取得するものを追加・変更したら、同じPRでこの表も更新すること（`AGENTS.md` のドキュメント同期ルール）。

## 取得する資産

| 資産 | 用途 | 取得先 | ライセンス | 取得コマンド |
|---|---|---|---|---|
| three.js | 配信ビューの3D描画 | 公式配布（CDN/リリース） | MIT | `cargo xtask setup viewer` |
| @pixiv/three-vrm | VRM の読み込みと表情・ボーン制御 | 公式配布（CDN/リリース） | MIT | `cargo xtask setup viewer` |
| 分割モデルの重み | `segment` 工程 | 採用モデル決定後に記載 | 決定後に記載 | `cargo xtask setup models` |
| 多視点生成モデルの重み | `multiview` 工程 | 採用モデル決定後に記載 | 決定後に記載 | `cargo xtask setup models` |
| メッシュ化モデルの重み | `mesh` 工程 | 採用モデル決定後に記載 | 決定後に記載 | `cargo xtask setup models` |
| 音声認識モデル | 表情の自動切替（発話のテキスト化） | 採用モデル決定後に記載 | 決定後に記載 | `cargo xtask setup models` |
| 小型LLM | 表情の自動切替（話題・感情の判定） | 採用モデル決定後に記載 | 決定後に記載 | `cargo xtask setup models` |

「決定後に記載」の行は [docs/TASKS.md](docs/TASKS.md) の未完了項目と対応する。
**ライセンスと再配布条件を確認する前に重みを既定の取得対象へ入れない。**

## Rust / JavaScript の依存

Cargo 依存のライセンスは `cargo license` などで列挙できる。配布前の監査手順は
`.agents/skills/audit-picovtuber-dependencies/SKILL.md` を参照。
