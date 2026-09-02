# SPEC.md — PicoVTuber 仕様

作業ルールは [AGENTS.md](AGENTS.md)。全設定キーは [docs/SETTINGS.md](docs/SETTINGS.md)。ここにはアーキテクチャ、コネクタのインターフェース、工程間の受け渡し形式、設定の保存・反映機構を書く。

## 1. 全体像

PicoVTuber は2つのモードを持つ。

```
[生成モード]  1枚のイラスト ──► 生成パイプライン（工程コネクタの列） ──► model.vrm
[配信モード]  model.vrm + マイク ──► 表情・口形の決定 ──► 描画 ──► 配信出力コネクタ
```

- バックエンドは Rust（`src-tauri/`）。フロントは webview（`ui/`）で、描画は three.js + three-vrm。
- **推論はすべて利用者のPC内で完結する。** 生成・音声認識・感情判定のいずれもクラウドAIサービスへ送らない。ネットワークを使うのは、公式配布元からのモデル重み・ランタイム・アプリ更新の取得と、利用者が明示的に始めたローカル配信連携だけ。
- 生成工程と配信出力はコネクタとして追加できる。`build.rs` が `src/pipeline/stages/*.rs` と `src/studio/outputs/*.rs` を走査して `#[path] pub mod` を生成し、各ファイルの `inventory::submit!` で登録される。中央の `mod.rs` を編集する必要はない。

## 2. 生成パイプライン

### 2.1 実行順の決め方

工程は自分が必要とする成果物を `requires()` で申告し、`pipeline::plan()` がその申告からトポロジカル順を決める。**実行順を配列で直書きしない。** 工程を1つ足すたびに離れた場所を直す作りになり、順序と依存がずれる。

- 循環依存、存在しない成果物への依存、同じ成果物を複数工程が名乗ること（正本の重複）は `pipeline::guard` のテストが検出して失敗させる。
- 各工程は入力を破壊せず、自分の出力を新しいファイルとして作業ディレクトリへ置く。したがって再実行は「その工程から後ろだけ」で成立する。

### 2.2 工程分類表

この表が工程一覧の正本。`doc_sync::guard` のテストが `src/pipeline/stages/` の実装と双方向で照合し、載せ忘れ・消し忘れを検出する。

| 工程ID | 役割 | 必要とする成果物 | 生成する成果物 | ランタイム | 機械学習 | GPU |
|---|---|---|---|---|---|---|
| `preprocess` | 入力イラストの正規化（余白トリム・等倍以下リサイズ） | （なし） | `source` | 不要 | なし | 不要 |
| `segment` | 前景と体パーツ（頭・髪・胴・腕・脚）の分割マスク生成 | `source` | `masks` | 必要 | あり | 任意 |
| `multiview` | 正面から側面・背面ビューを生成 | `source`, `masks` | `views` | 必要 | あり | 推奨 |
| `mesh` | 多視点から素体メッシュとテクスチャを生成 | `views` | `mesh`, `texture` | 必要 | あり | 推奨 |
| `rig` | VRM humanoid 標準ボーンの配置とスキニング | `mesh`, `masks` | `rig` | 必要 | なし | 不要 |
| `expression` | 表情ブレンドシェイプ（喜／怒／驚／悲／楽）の変形指示 | `rig`, `masks` | `expressions` | 不要 | なし | 不要 |
| `viseme` | 口形6種（`a` `i` `u` `e` `o` `n`）の変形指示 | `rig`, `masks` | `visemes` | 不要 | なし | 不要 |
| `pack` | VRM 1.0 として検証・組み立て・書き出し | `rig`, `expressions`, `visemes`, `source`, `texture` | `vrm` | 必要 | なし | 不要 |

「ランタイム」が「必要」の工程は `pipeline::runtime` 経由で PicoVTuber 管理下の Python を呼ぶ。**「ランタイムが必要」と「機械学習を使う」は別**で、`rig` と `pack` はメッシュを直接触るために Python を使うだけで推論はしない（GPU も要らない）。

機械学習を使う工程は、重みが未取得なら**その工程を失敗として返す**（正面の複製や白紙で埋めて成功に見せない）。

ボーン位置は推論ではなく、`segment` が作ったマスクの外接矩形から測る。頭のマスクの下端が首、脚のマスクの上端が腰、という具合に絵から測れるので、推論を挟むより安定し GPU も要らない。

### 2.3 `Stage` トレイト

```rust
#[async_trait]
pub trait Stage: Send + Sync {
    /// 工程ID（英小文字・数字・アンダースコア）。ファイル名と一致させる。
    fn id(&self) -> &'static str;
    /// 設定画面と進捗表示に出す日本語名。
    fn label(&self) -> &'static str;
    /// この工程が必要とする成果物の名前。
    fn requires(&self) -> &'static [&'static str];
    /// この工程が生成する成果物の名前。
    fn produces(&self) -> &'static [&'static str];
    /// 機械学習ランタイムを使うか（未導入時の案内と GPU 判定に使う）。
    fn uses_runtime(&self) -> bool { false }
    /// この工程の設定項目。設定画面へ自動で並ぶ。
    fn config_schema(&self) -> Vec<Field> { Vec::new() }
    /// 実行本体。入力は破壊せず、新しいファイルとして出力する。
    async fn run(&self, ctx: &StageContext) -> anyhow::Result<StageOutput>;
}
```

登録は各ファイル末尾で行う。

```rust
inventory::submit! { StageReg { make: || Box::new(Preprocess) } }
```

### 2.4 `StageContext` と `StageOutput`

`StageContext` は工程が触ってよい範囲を型で閉じ込める。

| 項目 | 内容 |
|---|---|
| `job_dir` | この生成ジョブの作業ディレクトリ（`SafePath`）。工程が書いてよいのはこの下だけ |
| `cfg` | 設定の参照。値はフィールドへ固定せず毎回 `cfg.get()` で読む |
| `inputs` | 先行工程が作った成果物（名前 → `Asset`） |
| `progress` | `0.0..=1.0` と一言メッセージを送る通知口 |
| `cancel` | 利用者が中止したかを見るフラグ。長い処理は定期的に確認して途中で戻る |

`StageOutput` は生成した成果物の一覧。`Asset` は「作業ディレクトリからの相対パス、種別、寸法または頂点数」を持つ。**絶対パスを外へ出さない**（ジョブディレクトリごと移動できるようにするため、そして診断ログへ利用者のフォルダ名を漏らさないため）。

### 2.5 成果物の命名規約

作業ディレクトリ直下に、工程IDのフォルダを作って置く。

```
<app_data_dir>/jobs/<ジョブID>/
├── input.png            ← 利用者が取り込んだイラスト（読み取り専用。工程は上書きしない）
├── job.json             ← 進捗の正本（どの工程まで完了したか）
├── preprocess/source.png
├── segment/masks/{head,hair,body,arms,legs}.png
├── multiview/views/{front,side,back}.png
├── mesh/{mesh.glb,texture.png}
├── rig/{bones.json,rig.glb}
├── expression/{joy,angry,surprised,sorrow,fun}.json
├── viseme/{a,i,u,e,o,n}.json
└── pack/{meta.json,model.vrm}
```

`expression/` と `viseme/` の JSON は `MorphSpec`（顔のどの領域を、どちらへ、どれだけ動かすか）。頂点の移動そのものは `pack` 工程がランタイムへ委譲して行う。変形指示を Rust 側に置くことで、**5種類と6形が本当に違う形になっているか**を書き出す前に検査できる（名前だけ違って中身が同じだと、切り替えても顔が変わらない）。

`job.json` は `store::write_json_atomic()` で書く。読めなかった場合は `store::quarantine_corrupt()` で退避してから空で続ける（空のまま書き戻して利用者のジョブを消さない）。

### 2.6 ジョブの再開

生成は時間がかかり、途中でアプリが落ちることがある。`pipeline::job` が工程単位で「完了した工程と、その成果物」を `job.json` へ記録し、再起動後は未完了工程から再開する。**完了扱いにするのは、その工程が `produces()` で申告した成果物が実在し、検証を通った場合だけ**とする（`job.json` の記録だけを信じない。書き込み直後に落ちるとファイルが無いのに完了になる）。

自動での再試行は通算5回までとし、利用者が明示した再試行では数え直す。

## 3. 配信モード

### 3.1 状態機械

```
未読込 ──[モデル読込]──► 待機 ──[入力開始]──► 追従中 ──[出力開始]──► 配信中
                          ▲                    │                        │
                          └────────[停止]───────┴────────────────────────┘
```

`start()` / `stop()` は冪等。二重起動・二重停止で失敗しない（配信中に利用者が同じボタンを連打しても壊れない）。

### 3.2 リップシンク（`studio/lipsync.rs`）

マイク入力を **PC内だけで** 解析して口形6種を決める。クラウドASRは使わない。

1. `cpal` で既定入力デバイスからモノラル `f32` を受ける。
2. 20ms のフレームへ切り、ハン窓をかける。
3. フレームの RMS が `PICOVTUBER_LIPSYNC_SILENCE_RMS` を下回れば `n`（閉じ）。
4. 上回れば、母音の弁別に効く3つの帯域（第1フォルマント帯 250–900Hz、第2フォルマント帯 900–2500Hz、高域 2500–4000Hz）のエネルギー比とゼロ交差率から `a` `i` `u` `e` `o` を推定する。
5. 口の開き量は RMS を `PICOVTUBER_LIPSYNC_GAIN` で正規化して `0.0..=1.0` へ収める。
6. 出力は指数移動平均で平滑化する（`PICOVTUBER_LIPSYNC_SMOOTHING`）。生の判定をそのまま出すと口がガタつく。

**音声は保存も送信もしない。** 解析はリングバッファ上で行い、フレームを使い終えたら捨てる。

### 3.3 表情（`studio/expression.rs`）

プリセット（`joy` `angry` `surprised` `sorrow` `fun`）とテキスト指定を、同じ重みベクトル（各 `0.0..=1.0`）へ落とす。切り替えは `PICOVTUBER_EXPRESSION_BLEND_MS` かけて線形補間する（瞬間切替は不自然に見える）。

口形と表情は**別のチャンネル**として合成する。表情が口の形を持つ場合（例: 大きく笑う）でも、口形側の重みを潰さない。

### 3.4 ローカルAIによる表情自動切替（`studio/local_ai.rs`）

同梱の音声認識（whisper 系のローカル実装）で発話をテキスト化し、同梱の小型LLMで話題と感情を判定して表情プリセットを選ぶ。**どちらも未導入なら機能を「利用不可」と表示し、クラウドサービスで代替しない。** 判定間隔と閾値は設定で変えられる。

### 3.5 配信出力（`studio/outputs/`）

```rust
#[async_trait]
pub trait Output: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// この出力が現在のOS・環境で使えるか。使えない理由は文字列で返す。
    fn availability(&self, cfg: &Config) -> Availability;
    fn config_schema(&self) -> Vec<Field> { Vec::new() }
    async fn start(&self, ctx: &OutputContext) -> anyhow::Result<()>;
    async fn stop(&self, ctx: &OutputContext) -> anyhow::Result<()>;
}
```

| 出力ID | 方式 | 対応OS |
|---|---|---|
| `transparent_window` | 背景を透過した専用ウィンドウを出し、配信ソフトの「ウィンドウキャプチャ」で取り込ませる | Windows / macOS / Linux |
| `virtual_camera` | 仮想カメラデバイスへフレームを流す | Windows / macOS |

`availability()` が「使えない」を返す出力は、設定画面で理由付きの無効表示にする。**選べるのに何も起きない状態を作らない。**

## 4. VRM の組み立て（`vrm.rs`）

出力は VRM 1.0。`pack` 工程は次を検証してから書き出す。1つでも欠ければ書き出さずに失敗を返す。

- humanoid の必須ボーンが揃っていること（hips / spine / chest / neck / head / 左右の upperArm・lowerArm・hand・upperLeg・lowerLeg・foot）。
- 表情プリセット5種と口形6種の morph target がすべて存在し、名前が VRM の標準表現名（`aa` `ih` `ou` `ee` `oh` `neutral` / `happy` `angry` `sad` `relaxed` `surprised`）へ対応付けられていること。
- テクスチャが参照可能で、寸法が入力イラストの等倍以下であること（引き伸ばし拡大の禁止。AGENTS.md 参照）。
- メタデータの利用許諾欄が空でないこと（利用者が取り込み時に答えた内容を入れる）。

## 5. 設定の保存・反映機構

- 正本は `src-tauri/src/config.rs`（永続ファイル `app_config_dir/config.json`）。優先順位は **永続ファイル > 環境変数 > ハードコード既定値**。
- コネクタは自分の設定項目を `config_schema()` で `Vec<Field>` として申告し、設定画面へ自動で並ぶ。値はフィールドへ固定せず、メソッド内で `cfg.get(キー, 既定値)` から都度読む。だから設定画面での保存はアプリ再起動なしに反映される。
- 書き込みは `store::write_json_atomic()`（一時ファイル → `sync_all` → `rename`）。内容が1つも変わらない保存はファイルへ書かない。
- `type="password"` の値は平文で置かず、`secret.rs` がOSの資格情報保護へ預ける（Windows: DPAPI、macOS: キーチェーン、Linux: 平文＝フォールバック許可）。復号できない保護形式が残る値は未設定として扱う。
- 全設定キーの一覧は [docs/SETTINGS.md](docs/SETTINGS.md) が正本。`doc_sync::guard` のテストが実装と双方向で照合する。

## 6. 設計ガード（テスト専用）

実装規則のうち、レビューで見落とすと実害が出るものは機械検査にしてある。走査範囲は `guard_scan.rs` が一元管理し、各ガードが自前でディレクトリを歩くことは `guard_scan::guard` のメタガードが禁じる。

| ガード | 検出するもの |
|---|---|
| `store::guard` | 状態ファイルの `fs::write` 直接使用、破損時に退避もエラー返却もしない「空で置き換え」 |
| `text::guard` | 固定長バイト添字での文字列切り出し、ストリーミング受信での直接 lossy 変換 |
| `tools::process::guard` | `Command::new` の直接使用、自前 `hide_window`、自前 `timeout(..., wait())` |
| `permissions::guard` | 自前の許可判定、パス引数の `resolve` 漏れ、`SafePath` への `std::fs::write` |
| `pipeline::guard` | 工程の循環依存、存在しない成果物への依存、成果物の正本重複、実行順の直書き |
| `doc_sync::guard` | `docs/SETTINGS.md` と実装の設定キーのズレ、工程分類表と `stages/` のズレ |
| `secret::guard` | 秘密っぽいキー名なのに `password` 型でない申告、`is_secret_key` に載らない password 項目 |
| `cloud_free::guard` | クラウド推論サービスのエンドポイントらしき文字列がソースへ入ること |
