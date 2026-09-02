---
name: audit-picovtuber-dependencies
description: PicoVTuberのCargo依存、JavaScript・Pythonのパッケージ、同梱または管理導入するPythonランタイム・生成モデル重み・音声認識モデル・小型LLM・three.js／three-vrmを、all／packages／assets等の指定範囲で棚卸しし、公式最新版・互換性・ハッシュ・ライセンス・文書・取得実体まで更新検証する依頼で使う。
---

# PicoVTuberの外部依存を監査する

## 対象を決める

1. 引数を `all`、`packages`、`assets`、または個別カテゴリとして扱う。省略時は `all` にする。
2. `packages` はCargo、Python、その他のパッケージマネージャー管理物を対象にする。
3. `assets` は管理導入・取得するランタイム、生成モデル重み、音声認識モデル、小型LLM、Webライブラリを対象にする。
4. 個別指定では `cargo`、`python`、`viewer`、`runtime`、`models` を受け付ける。

## 棚卸しする

1. `Cargo.toml` / `Cargo.lock`（本体・xtask・`crates/`）、`xtask/src/setup.rs` の取得定義、`THIRD_PARTY_NOTICES.md` を突き合わせる。
2. `rg` で `VERSION`、`releases/download`、`releases/latest`、`resolve/main`、`sha256`、`integrity`、CDN URL を再検索し、棚卸しの漏れを点検する。
3. Git管理外の `ui/vendor/`、`src-tauri/resources/bundled/`、`app_data_dir/models/` の実体も、版マーカーまたはハッシュで確認する。
4. 大容量取得の呼び出し元を逆引きし、**起動・通常ビルド・引数省略・インストール後処理が任意モデル・ランタイムを暗黙取得していないか**確認する。
5. OS、CPUアーキテクチャ、GPU種別、必要能力を取得前に判定しているか確認する。CUDA版はCUDA対応NVIDIA GPUだけを対象とし、AMD／Intel／GPUなし環境では取得しない。CPU版を選ぶのは対象機能がCPU実行を正式対応する場合だけにする。
6. PicoVTuber管理下に旧版、退避、staging、`.part`、`.incoming`、`*-old-*`、成功済み処理の再開キャッシュが残っていないか確認する。

## 公式最新版と照合する

1. GitHub Releases、crates.io、PyPI、各公式配布元など一次情報だけを使う。
2. 公式LTSまたは長期保守安定版がある場合はLTSを優先し、無い場合は安定版latestを採用対象とする。nightly、preview、RCは明示指示がない限り採用しない。
3. 「最新版の版」と「PicoVTuberが実際に取得する版」を分けて記録する。旧版を維持する場合は固定理由と確認日を記録する。
4. `latest` 追従も合格扱いにせず、現在解決される版、アセット名、対象OS、サイズまたはdigestを確認する。
5. **生成モデル・音声モデルはLTSを選ばず公式の最新安定版へ追従する。ただし採用前に必ずライセンスと再配布条件を確認する。** 条件が不明なものは既定の取得対象へ入れない。
6. CUDAを使うPython依存は、対象GPU・CUDA・PyTorch・CUDA拡張の公式wheelが同時に対応する組合せへ統一する。現在の基準はPython 3.12。

## 更新する

1. パッケージ更新と資産更新はユーザー指定に応じて同じPRまたは目的別PRへ分ける。
2. Cargo更新では各ロックを更新し、直接依存のメジャー更新も確認する。`Cargo.toml` または `Cargo.lock` を変えたら各ロックへ `cargo audit --file` を実行する。
3. 資産更新では固定版、取得URL、アセット選択、ハッシュ検証、版マーカー、対応OSを同期する。
4. 条件別アセットでは、非対応環境が**ダウンロード開始前に**停止または対象を除外する実装と回帰テストを追加する。取得後の起動で失敗させない。
5. 新版の検証成功後に旧版と一時物を削除し、正常終了後の管理領域に不要物が0件であることを確認する。
6. 現行情報を `THIRD_PARTY_NOTICES.md`、`README.md`、`docs/DEVELOPMENT.md` へ反映し、監査結果をPR本文へ書く。未完了・条件待ちだけを `docs/TASKS.md` へ残す。

## 毎月点検する

1. 毎月1日に、このリポジトリを分離worktreeで開き、`audit-picovtuber-dependencies all` を実行するよう登録する。
2. 更新があれば目的別ブランチで修正、検証、PR、squash mergeまで行う。更新が無ければ変更を作らず、点検結果だけを報告する。

## 検証する

1. 変更した各マニフェストへ `cargo check`、対象テスト、必要な `cargo clippy -- -D warnings` を実行する。
2. 取得処理はURL組立、OS別・GPU別アセット選択、ハッシュ拒否、版比較のテストを通す。
3. PR本文へ対象、現行版、上流版、更新・見送り理由、監査結果、未確認事項を書く。
4. 正常終了後に管理領域の旧版・一時物・完了済みキャッシュが0件であることを検索結果で確認する。
5. 完了報告では「packages」「assets」を分け、各カテゴリの採用版と保留を明示する。

## 停止条件

- 一次情報から最新版または配布条件を確認できない。
- 更新後に対象OSのアセットが存在しない。
- ハッシュや署名を検証していた経路が弱くなる。
- **生成モデルのライセンスまたは再配布条件が不明**。
- 実体の版を確認できないまま配布ビルドへ進もうとしている。
