# DEVELOPMENT.md — 開発・ビルド・配布

## 通常の局所補完

通常完成リグは`rig-generations/g_<ID>/`へ同期し、`rig-current.json`だけを原子的に置換する。読者はWindows OSロック付きleaseで1世代を保持する。current・previous（直前1世代）・生存読者の世代を残し、不要世代と終了leaseを回収する。Windows起動時もRustだけで参照と回収を処理し、Pythonを無条件起動しない。旧参照がないlegacy形式に限り、Python互換の集合/キャラbyte0排他を取得して欠落したrig2dを退避版から復旧する。別プロセス稼働中は警告し、pendingの昇格や工程状態の変更をしない。全工程の起動時復旧ではない。旧比較`rig2d/`は移動・削除しない。

ブラウザ確認は固定世代のNDJSON snapshotを逐次受信し、素材SHA・PNG/RGBA・原寸・URL・終了レコードを検証してBlob URLを共通レンダラーへ渡す。配信中断ではleaseと未採用Blobを解放し、旧表示・選択名を保持する。既定上限はレコード0.096 MB、チャンク0.048 MB、1素材32 MB、合計256 MB、256素材、辺長8192px。`display.snapshot_*`の保存値を読み、上限超過を縮小で通さない。サーバーの全体応答bufferを避けるが、ブラウザのBlob・デコード・WebGLメモリは別に必要で、全体の省メモリ実測は未完了。WebGL構築失敗・コンテキスト消失からの復旧までは保証しない。

最終補完版4は閉眼に加え、原画耳のDINO/SAM解析→隠れ顔編集→横髪/耳編集→生成耳のDINO/SAM解析を逐次実行する。追加編集はマスク版2のscene髪所有・閉領域・限定境界（横髪/耳は測定原画耳を加える）と元画像の潜在表現を使い、目口とマスク外を保持する。構図が変わった全体再生成をそのまま貼らない。`completion-hidden-source/`・`completion-side-source/`と`completion-original-ears/`・`completion-generated-ears/`を独立保存し、抽出だけの失敗で推論を繰り返さない。両側の原画耳が測れない場合は可視輪郭の描き直しを抑止し、partialと画面の警告を残す。生成耳が両側測れなければ明示失敗する。追加2回のQwen推論で所要時間が増えるため、閉眼だけの実測時間を全補完の所要時間としない。統合のCPU検証と実機品質検証は別に行う。

作業用の`tools/capture-character-gallery.mjs <characterId> <temp内の出力先>`は起動済み8791の共通確認画面をChromeで撮影する。Node/Playwrightは開発作業用だけで製品依存ではない。既存Playwrightを使う場合は`LVS_PLAYWRIGHT_MODULE`へその`index.mjs`を指定する。中立・左右閉眼・母音・小角度・連続動作と撮影ハッシュを保存し、撮影用Chromeを終了する。ハッシュ差を品質合格と扱わず実画像を目視する。

末尾に`--video`を付けると静止画も従来どおり残し、口パク・自動まばたき中の共通レンダラーのcanvasを8秒の無音`motion.webm`へ録画する。要求値は20 fpsだが、実際のフレームレートは描画負荷に依存するため`requestedFps`と区別する。headless Chromeを使用し、別canvasや生成画像で動きを作らない。録画機能非対応・空データ・描画エラーは明示失敗とし、ページとブラウザを終了する。録画形式・寸法・時間・SHAを`capture-report.json`へ保存する。WebMの透過再生は閲覧ソフト依存なので、透明境界の合格は別の市松静止画で検証する。

```powershell
node tools/capture-character-gallery.mjs c_df28cf7d4d11 temp/character-video-test --video
ffmpeg -n -i temp/character-video-test/motion.webm -vf "fps=12,scale='min(480,iw)':-1:flags=lanczos" -loop 0 temp/character-video-test/motion.gif
```

GIFはスマホ向けの動作確認用に縮小するだけで、原画・最終素材に戻さない。色数や透過表現は元のWebMと異なり、静止画の原寸品質検査を代替しない。

現行の正規入口はアプリの「全工程を実行」と`pipeline-probe`で、`isolate → decompose → rig2d → complete`を逐次実行する。DINO/SAMの解析を維持し、未補完リグを`rig2d-base/`、Qwen-Image-Edit-2511の閉眼・隠れ顔・耳を適用した最終リグを`rig-generations/`へ分離し、`rig-current.json`で公開する。口は現状承認済みで、この工程から描き直さない。PicoAgent本体への組み込み、OBS、VRM、macOS対応は現在の作業対象外とし、旧PoCは比較用に保持する。

```powershell
cargo run -p local-vtuber-studio --bin pipeline-probe -- <input.png> <id> <identity-tags>
cargo run -p local-vtuber-studio --bin pipeline-probe -- --resume <characterId> rig2d
cargo run -p local-vtuber-studio --bin pipeline-probe -- --only <characterId> complete
```

`sidecar/completion/generate.py`はRustから呼ぶ製品サイドカーであり、`tools/qwen-eval/`の比較入口とは分離する。`sidecar/completion/models.json`の固定重みを検査し、管理下ComfyUIだけを127.0.0.1で起動する。カスタムノード・APIノードを無効にし、既存利用者ComfyUIの追加モデル設定を読まない。Pythonは共有3.12環境を使い、GPU処理を並行起動しない。

モデル配置は`ai.models_dir`基準の`ai.completion_model_dir`（既定`models/qwen-eval/`）。開発取得入口は`cargo xtask setup completion`で、`tools/setup-completion-models.py`が採用済みImage-Edit・共通テキストエンコーダ・VAEの3ファイルだけを固定SHA検査後に配置する。既存の同一重みは再取得しない。比較用の`tools/qwen-eval/download.py`はLayeredを含む5ファイルの別入口であり、通常セットアップには使わない。モック取得検査とcargo checkは通過したが、新規環境への実ダウンロード・配布セットアップの検証は別途必要である。

設定画面の「Qwen局所補完の設定」から保存し、次回の補完で読む。全キー・範囲・既定値は[SETTINGS.md](SETTINGS.md)を正本とする。原寸頭部ROIの目マスクだけを編集し、拡大素材を採用しない。このPCの比較実測では閉眼1体約15〜18分、RSS最大約16.3 GB・GPU全体最大約7.6 GB。通常入口の実走・全キャラ品質の証明とは分ける。

`completion-source/{edited.png,manifest.json}`と隠れ顔/耳の2rawは、生成署名と抽出署名を分離する。閉眼生成版3・隠れマスク版2を維持する。実入力PNG/マスク・原寸ROI・原画2SHA・モデル・ComfyUIコード/依存版・確定workflow・全条件が厳密一致すれば、解析2SHAだけの変更では再推論しない。元raw manifest全バイトと実生成時source4SHAを保持し、今回のsource4SHAと別に完成証跡へ記録する。RawLeaseが画像とmanifestの取得時SHAを固定し、後工程後・読込時・公開直前に再照合する。原画/解析/基底/コードの実行中変更は拒否する。既知eye1/2・隠れmask1は完全性検査後に新版不一致として再生成、未知版/破損は明示失敗。実入力準備や推論の意味を変えた場合だけ生成版を上げる。

公開世代の`completion.json`には最終版4の現在source4SHA・基底・元rawの出自/画像SHA・抽出コード・完成素材SHAを記録する。公開前SHA検査を外さない。成功後は処理専用一時ディレクトリを除去し、失敗時は診断と成功済みrawを保持する。partialは`rig.local_completion.hidden.warning`へ保存し、最終cache再利用時も画面へ警告する。初回移行で旧mutable rig2dを新方式の完成cacheとして採用せず、検証済みrawから抽出して世代公開する。

閉眼抽出では主曲線を変えず、測定列厚さ内にある薄い/分離した睫毛片を下地から除く。原寸・許可域外・アルファ・白目を保持し、空線/非有限/参照肌不足は明示エラーにする。むぎの原寸抽出候補はChrome半閉眼/全閉眼を確認したが、通常再生成後と別原画の目視は別の受入条件である。

WindowsのPythonサイドカーは停止状態で起動し、所有するJob Objectへ所属させてから再開する。Jobのkill-on-closeにより正常終了・中断・Drop・不正JSON時にComfyUIを含む子孫も回収する。起動からJob所属までの極短区間にアプリをOS強制終了すると、停止中Pythonだけが残る可能性はある（GPU初期化前）。既存利用者プロセスを名前で一括終了しない。

本体の通常プレビューも公開参照と読者leaseから固定世代を取得し、SHA・素材寸法・容量を検査する。生成中/失敗後でも公開済み世代を読めるが、未公開の失敗出力を許可しない。旧比較形式は従来の状態照合を残す。正規生成はPipelineContext経由とし、手作業で世代ディレクトリやrig-currentを改変しない。

袖・手の可視分割は解析署名版3。`--resume <characterId> decompose`で同じDINO/SAMの追加問い合わせと選別を実行し、analysis/manifestのoptional_limbs状態を確認する。左右同側の腕から可視所有だけを移管し、曖昧候補は記録して採用しない。scene素材は親腕の変位を継承し、独立関節/隠れ素材の完成とは扱わない。解析2SHAが変わっても3rawの実入力が同一なら再利用できる。

本体の素材読込はリグの`layers`を正本にし、追加した隠れ顔・耳素材も取り込む。必須素材の欠落、安全でない識別子、正規形式以外のURL、リンク/reparse経路は拒否する。URLを外部取得先として使わず、キャラ配下のPNGだけを読む。実symlink検査は作成特権が必要なため、このWindows環境では未検証（特権不足による明示ignore）。

以下の補完候補の節は比較履歴の再現手順であり、生成済み候補を通常完成出力へ手動コピーする入口ではない。通常入口の検証では同条件で複数原画を通し、原画保持、半閉眼、口パク、首・襟、透過を実表示で確認する。

Rust と Tauri CLI だけで開発起動・テスト・Windows配布ビルドを行う。Node.js は不要。

描画の開発用回帰検査は `node --test tools/test-mouth-geometry.mjs tools/test-eye-geometry.mjs tools/test-rig-motion.mjs tools/test-texture-alpha.mjs tools/test-avatar-lifecycle.mjs tools/test-native-scene-batch.mjs`。Node.jsは作業用のみ。同じ変位場の連続部位を`native-scene-batch.js`で原寸合成し、hidden_face/独立髪は境界として順序を保つ。顔のCPU再合成範囲と、GPUへ転送する合成テクスチャ全体を混同しない。口内の上歯はクリップ内・原画の唇より奥に描き、閉口と丸めた母音の回帰を検査する。

## 補完候補を従来の動作確認へ追加する

確認サーバーの`/api/normal-characters`は通常4工程を完了し`completion.json`があるキャラのID・表示名だけを列挙する。確認画面は再読込時に選択肢へ追加し、内部設定やプロンプトは一覧へ出さない。素材ロード後に完了世代を再照合してから表示する。これはPipelineContextが先に状態を更新する前提の世代検査で、独立したsidecar直接実行との原子的な読取保証ではない。

閉眼比較版7は、暗線の8近傍成分から列の局所厚さ・連続横幅・成分保持率を検査し、傾きや目尻の長い枝を全体高さだけで拒否しない。`curve_detection`へ採用列数・厚さ・測定範囲を残す。eye_baseは生成済みの肌を使い、検出線と縁だけを周囲の肌から調和補間して線を分離する。参照肌不足・暗線除去失敗は拒否し、`skin_reconstruction`へ範囲と明度差を記録する。白目用eye_backplateは肌へ変えない。`sidecar/.venv/Scripts/python.exe tools/qwen-eval/test_closed_preview.py`で採用・拒否・保護領域・素材分離を検査する。既存候補を上書きせず署名版を変更する。

比較仕様版19のむぎ候補IDは`c_2379190bb3b3`。利用者承認により可動モデルの耳輪郭を描き直す。原画ファイルは保持するが、中立合成も許可した耳周辺26,120画素が変わる。目口と許可領域外の変更はエラーにする。耳の形で進めることは利用者承認済み。髪との接合部の品質は未合格。

輪郭修正の前に、既存のDINO baseとSAM2.1 Hiera Tinyで生成画像・原画の耳を別々に抽出する。モデルはローカル固定配置から順次読み込み、ダウンロードしない。

```powershell
sidecar/.venv/Scripts/python.exe tools/qwen-eval/segment_ears.py temp/qwen-eval-edit-mugi-side-ear-bf16 --character temp/t7-characters/c_190454c86edb
sidecar/.venv/Scripts/python.exe tools/qwen-eval/segment_ears.py temp/qwen-eval-edit-mugi-side-ear-bf16 --character temp/t7-characters/c_190454c86edb --source-reference
sidecar/.venv/Scripts/python.exe tools/qwen-eval/build_preview.py --character temp/t7-characters/c_190454c86edb --comparison temp/qwen-eval-edit-mugi-head-bf16 --side-comparison temp/qwen-eval-edit-mugi-side-ear-bf16 --redraw-ear-contour
```

完了した`ears/`と`source-ears/`には原寸マスクと入力SHAを保存し、既存出力は上書きしない。`--threshold`既定0.2、`--context`既定0.5は検出閾値と耳検出枠に対する解析余白である。生成耳は左右とも必要。原画側は隠れて検出できない側を欠測として記録するが、両側欠測はエラーにする。今回の実測は生成側17.39秒、原画側5.875秒。新しい耳を顔素材に合成し、古い耳は未分類を含む全素材から除く。顔の基準座標は保持し、素材の切り出し範囲だけを拡張する。耳全体を明度差で選ぶ案は背景まで矩形で混入したため却下した。

`--side-comparison temp/qwen-eval-edit-mugi-side-ear-bf16`を指定すると、同じ原画/解析/原寸範囲で完了した横髪除去結果を使用する。実測した目の高さで前髪用下地から耳・頬用下地へ滑らかに切り替える。`--edge-band-ratio`（既定0.015、0超〜0.05）は髪境界の調整幅/顔幅、`--edge-gain`（既定40、0超〜255）は横髪除去での平均RGB明度増加の下限。髪に接続する境界だけを再分類し、目口と口より下の輪郭を保護する。むぎでは637画素を顔側から髪へ戻した。全素材は原寸のままであり、画像名による分岐はない。この境界調整の汎用性は未承認。

`--redraw-ear-contour`なしの比較経路もPoCとして残す。この経路の`hidden_motion.repair_layer`が指す`scene_ear_repair`は、目の中央高さから口の上までの髪に隣接する可視境界帯に限定する。目口を除外し、原画からの距離でアルファを減衰する。共通レンダラーは中立で補修を表示せず、顔左右/上下の絶対角度をangle_limitで割った強さで合成する。比較チェックで下地と動作時補修を隠す。一方、輪郭修正版は耳を顔素材へ直接合成するためチェックでは耳が戻らない。元キャラへの切り替えで比較する。

比較専用入口は`sidecar/.venv/Scripts/python.exe tools/qwen-eval/build_preview.py --character temp/t7-characters/<元ID> --comparison temp/qwen-eval-edit-<比較名>`。完了済み原寸頭部Image-Editと原リグが必要。新しい推論はせず、生成済み画像を局所下地へ加工する。原画/解析の4つのSHA、原リグと全PNG、補完画像を公開前に再照合し、変更があれば候補を公開しない。同一候補の上書きは拒否する。

`--band-ratio`は顔の実測幅に対する補完帯（既定0.08、0超〜0.15）、`--motion-ratio`は帯幅に対する局所変位量（既定0.35、0超〜0.4）。これらは比較ツールの引数で、製品の永続設定へ採用していない。仕様・ハッシュ・引数から候補IDを作り、`temp/t7-characters/`へ原本のコピーとリグを原子的に公開する。出力IDを確認ページの比較一覧へ登録する。素材はGitに含めない。

原画の不透明な髪に隠れた顔の近傍だけを生成下地とし、可視肌との差を正規化畳み込みで滑らかに補正する。全素材で髪の所有画素の重複を候補内だけ整理する。非描き直し経路は中立RGBA一致を、輪郭修正版は許可領域外・目口の一致を検査する。生成下地は`scene_hidden_face`、検証条件は`experimental_hidden`、共通格子65×97の局所重みと変位限度は`hidden_motion`へ保存する。変更時は比較仕様の版を上げる。

共通レンダラーで候補の髪だけ独立頂点を持たせる。他の部位は従来の連続変位場を維持し、未補完の首肩・外周まで独立回転させない。`/ui/check.html`で元キャラと切り替え、顔左右/上下、補完表示のオン/オフ、口、左右別まばたきを確認する。中立の数値一致は動作時の髪際や画質の合格ではない。髪際の線・閉眼/口の造形は未合格。

比較仕様版19は顔と未分類の両方から髪境界637画素を回収する。髪に完全に囲まれた領域7,934画素（髪飾り・ハイライト等）も、目口を含む閉領域と透明画素を除外して髪へ移す。色は原画のままで、髪だけ動いて飾りの境界が取り残される現象を抑える。狭い色差条件と閉領域条件の汎用性は別原画では未検証。

耳輪郭修正版では、まばたき用の左右eye_base/eye_backplateにも髪・耳修正領域の所有マスクを適用する。シーン素材だけ直すと、閉眼途中に古い耳や矩形状の髪が再表示されるため、静止確認だけでは検証完了にしない。

版19では肌色参照マスクを収縮して暗い輪郭を除き、色補正後の整数化を丸めにして定常色の切り捨て誤差を避ける。

### 原寸閉眼素材を動作候補へ追加する

固定1024px範囲では小さな原画の全身まで入り、Image-Editが頭部へ構図を変更した例がある。`run.py edit --head-framing measured`で顔・首の実測範囲に基づく等倍の頭部切り出しを比較できる。最小256px、16の倍数、`--resolution`を上限とし、上限や原画の寸法に収まらなければ明示エラーにする。画像を縮小・引き伸ばしせず、範囲と方式を署名情報へ保存する。これは比較オプションで、既存候補を再生成・上書きしない。生成画像が構図を維持したかを目視し、元の座標で閉眼線が測れなければ公開しない。

耳修正は閉眼補完の前提ではない。通常キャラの場合は`--base`と`--character`へ同じキャラを渡す。派生した補完候補の場合は`experimental_hidden.source`の署名一致も必須とする。原画一致だけで別の解析結果のリグを受け入れない。

新規の閉眼比較は版5で入力範囲の方式と編集マスクも署名対象にする。左右のeye_base/eye_backplateは髪の所有画素を除外し、局所編集マスクでアルファを制限する。髪マスクの漏れがあっても目の編集範囲外へ肌を重ねない。色・原寸は保持する。既存の承認済みむぎ（版2）は再生成せず維持する。

`run.py edit --head-framing measured --edit-region eyes`では`qwen-edit-eyes-overlay.json`を基本ワークフローへ重ね、原画VAEEncode→局所ノイズマスク→生成→元画像へのマスク合成を行う。`--mask-margin-ratio`は目幅に対する余白で既定0.2、0超〜0.5。髪・透明画素を除外し、原寸マスクのSHAを記録して終了時も再検査する。新規カスタムノードは不要。出力は完了後も目視検証が必要で、フレーミングや閉眼線が不適合なら取り込まない。肌のアルファ254を透明として全除外しないが、元のアルファを255へ書き換えることもしない。

完了した同一原画の原寸編集と耳修正候補を入力する。比較仕様版2の出力`c_df28cf7d4d11`は耳・閉眼の見た目について利用者承認済み。既存出力は上書きしない。

```powershell
sidecar/.venv/Scripts/python.exe tools/qwen-eval/build_eye_preview.py --character temp/t7-characters/c_190454c86edb --base temp/t7-characters/c_2379190bb3b3 --comparison temp/qwen-eval-edit-mugi-closed-eyes-bf16 --allow-unmasked-comparison
```

生成画像から閉眼曲線と暗いまぶたの透過素材だけを抽出する。肌ごと重ねる版1は半閉眼で二重線を生じたため不採用。承認済み版2では左右上まぶたPNGとリグ定義だけを変更し、原画・肌下地・他PNGは親と同一に保つ。新規版5では前述の目の肌下地のアルファ制限も行い、他素材は保持する。入力4SHA・親素材・生成画像を公開前にも照合し、原子的に別候補へ保存する。閉眼曲線が測れない場合はエラーにする。左右別に開き0/0.5/1と連続まばたきを確認する。拡大生成や口の変化を持ち込まない。

## 必要なもの

### 原寸閉眼のローカル編集診断

`build_eye_preview.py`は通常、目の局所編集とマスクSHA・マスク外画素保持を必須とする。非限定の旧比較は構図を目視確認してから`--allow-unmasked-comparison`を明示する。女性Aで失敗した非限定出力へこの許可を付けてはならない。

`tools/evaluate-eye-inpaint.py`は採用済みAnimagine XL 4.0の固定SHAを検査して使う比較専用ツールであり、製品パイプラインへ候補を昇格しない。作業用Pythonから`--character <キャラフォルダ> --comfy <本アプリ管理ComfyUI> --output temp/<新規診断名>`を指定する。任意の`--denoise`、`--mask-grow`で条件比較する。既存利用者環境や`extra_model_paths.yaml`のある環境を使わない。

元キャンバスへVAE倍数の余白だけを足し、リサイズせず目マスク内を編集する。出力は診断領域のcandidate.png/report.json/workflow.jsonとログのみで、原画・正規素材を変更しない。起動するComfyUIは127.0.0.1の専用ポート、API/カスタムノード無効、処理後に終了・waitする。同時にほかのGPU処理を走らせない。成功ログを品質合格と解釈せず、虹彩色の閉眼への混入などを原画と目視比較する。

### 意味解析候補の比較

承認済みQwenの比較準備は`sidecar/.venv/Scripts/python.exe tools/qwen-eval/download.py`を使う。`models.json`の5ファイルだけを固定版で取得し、SHA-256を照合してから`models/qwen-eval/`へ公開する。途中ファイルは`temp/qwen-download/`に置き、Rangeの範囲・長さを検査して再開する。各取得ファイルの応答を4個までに制限し、全重みをメモリへ蓄積しない。

比較実行は`sidecar/.venv/Scripts/python.exe tools/qwen-eval/run.py layered --character temp/t7-characters/<ID> --output temp/<新規比較名> --comfy <本アプリ管理ComfyUI>`。もう一方は`layered`を`edit`へ変更する。既定は同じ原寸1024pxの頭/首ROI、50step、seed777。`--view full`は比較専用の長辺1024px以下への縮小で、拡大は行わない。`--prompt`で比較条件を明示的に変えられる。既定のEditは髪除去と隠れた顔/首/服の補完、Layeredは内容記述から4レイヤーへ分解する。**用途が異なるため、出力枚数を品質の順位と扱わない。**

専用localhostポートでComfyUIを起動し、APIノード/カスタムノード/Hub通信を無効化する。生成は逐次、DynamicVRAMで非量子化重みを必要時に読み込む。`--fast-disk`はNVMeからの動的読込を優先する比較指定であり、精度や解像度は変えない。ワークフローの正本は`workflows/qwen-layered-api.json`と`qwen-edit-api.json`。入力範囲、原画・透過原画・解析JSON・マスクのSHA、実行ワークフロー、全出力（Layeredの0枚目の全体再生成も含む）、ログ、プロセスRAMとGPU全体使用量の時系列を保存し、終了/失敗時に起動したプロセスツリーを回収する。GPU全体使用量には他アプリを含み、プロセス専用VRAMと混同しない。品質判定後も診断素材を正規リグへ無断で昇格しない。

完了出力は`sidecar/.venv/Scripts/python.exe tools/qwen-eval/analyze.py temp/<比較名> --character temp/t7-characters/<ID>`で解析する。生成時の原画/透過原画/解析/マスクのSHAが一致しない場合は拒否する。Layeredの全体再生成と残りレイヤーの合成誤差、入力との差、目口等の可視画素差、メモリピークを`metrics.json`に保存する。画素差は形状や造形の良否を証明しないため、見た目の判定は別に行う。

ブラウザで見せる比較の出力名は`temp/qwen-eval-<英小文字・数字・ハイフン>`にする。確認サーバーの`/ui/qwen-check.html`は、その範囲のreport.jsonに記載されたPNGとreference/recomposedだけを配信する。Layeredの0枚目を「全体再生成」と表示し、原画や独立素材へ誤分類しない。任意のtempファイル、重み、ログ、レポートそのものは公開しない。新しい結果は「結果を更新」で読み直す。

`tools/semantic-eval/inspect-components.py`は正規解析で保存した衣服の検出矩形を同じSAMへ渡し、最大連結領域と全領域を比較する。原寸マスクと上位の成分画像・面積を`temp/clothing-components/`へ保存する。左右に離れた衣服をノイズとして捨てていないかを確認する診断であり、その画像を製品へコピーしない。

正規出力の透明度保持は `sidecar/.venv/Scripts/python.exe tools/verify-scene-alpha.py temp/t7-characters/<ID> ...` で検査する。全キャンバスの `scene_*` 部位をsource-over合成し、背景除去原画との差があれば失敗する。これは原寸アルファ検査であり、RGB同一性・画面のフィルタリング・動作時の品質は別途確認する。

`segment.py --roles eyes --box-context 0.5`は、検出矩形の各辺へ幅/高さの50%を足した原寸ROIを同じSAMへ入力する局所解析の比較である。出力を`box-context-0.5/`へ分離し、sampling_regionを記録する。候補矩形や元画像を変更せず、モデル内部の解析解像度と最終素材の原寸を混同しない。全頭部解析と原寸マスクを比較し、背景/髪の混入も調べてから通常経路への採否を判断する。

細部比較では`tools/semantic-eval/evaluate.py dino --model-path models/grounding-dino-base --run-name <診断名> --labels eyebrow "eye pupil" --view head --measured-head`を使える。`--measured-head`は正規解析の顔座標を参照し、頭部比率の固定切り出しを使わない。候補の語句・座標・スコア・クロップ・所要時間を診断JSONに残す。`segment.py --run-name <同じ診断名> --roles eyebrow "eye pupil"`で既存SAM2へ渡す比較ができる。左右の細部を確定できない場合は明示失敗にし、未検出の原画を成功扱いしない。これらは比較専用で、検出候補を製品リグへ自動採用しない。

通常経路のブラウザ検証はリポジトリルートで `sidecar/.venv/Scripts/python.exe tools/preview_server.py` を起動し、`http://127.0.0.1:8791/ui/check.html` を開く。原画と本体共通レンダラーを比較する。`--lan`は信頼できるLANでのスマホ確認に限る。サーバーは画面・共通描画JS・検証用キャラの画像/JSONだけを許可し、モデル・プロジェクト文書・ディレクトリ一覧を返さない。応答はno-storeとし、旧モジュールのキャッシュがある場合は版付きURLで開き直す。GPU生成後にまとめて確認し、使い終わった自分のサーバーだけを停止する。

SAM2の画像マスク推論は共有環境のSam2VideoModelを重み整合性検査付きで使う。Transformers 4.57.6の単フレームbox+points併用にはnum_objects未初期化の問題があるため、髪の矩形を公式VideoProcessorと同じ角ラベル2/3へ変換し、目口の除外点0と同じ入力へまとめる。重みや共有ライブラリを改変せず、複数マスク推論を維持する。

利用者が比較を承認したFlorence-2-large-ftとGrounding DINO baseを `tools/semantic-eval/` で再現する。比較後にGrounding DINOの組み込みが承認され、通常経路は `cargo xtask setup grounding`（`setup models`にも含む）で `models/grounding-dino-base` へ固定版・SHA256検証付きで取得する。比較用の保存先とは分離し、診断画像を製品成果物へ転用しない。共有Python 3.12/Transformers 4.57.6を使い、Florence比較に必要なtimm 1.0.29（Apache-2.0）だけを追加する。既存torch等を更新しない。

```powershell
uv pip install --python sidecar/.venv/Scripts/python.exe --no-deps -r tools/semantic-eval/requirements.txt
sidecar/.venv/Scripts/python.exe tools/semantic-eval/download_models.py
sidecar/.venv/Scripts/python.exe tools/semantic-eval/evaluate.py florence
sidecar/.venv/Scripts/python.exe tools/semantic-eval/evaluate.py dino
sidecar/.venv/Scripts/python.exe tools/semantic-eval/segment.py --backend Florence-2-large-ft
sidecar/.venv/Scripts/python.exe tools/semantic-eval/segment.py --backend grounding-dino-base
sidecar/.venv/Scripts/python.exe tools/semantic-eval/summarize.py
```

GPU工程は逐次実行する。原画は `temp/t7-characters/<id>/source/isolated.png`。evaluate/segmentの `--characters <id> ...` で別原画にも同じ処理を適用する。既定3件は比較fixtureであり、キャラ別ロジックではない。`evaluate.py --run-name repeat` を付けて反復し、推論結果の一致を集計する。モデルは `models/semantic-evaluation/`、取得manifestと画像・数値結果は `temp/` 内へ保存する。描画確認用の縮小画像を高精細素材へ採用しない。

| 比較モデル | 固定リビジョン | 重みSHA-256 |
|---|---|---|
| Florence-2-large-ft | `4a12a2b54b7016a48a22037fbd62da90cd566f2a` | `8b4e610c952eef90a836c56cda0f398a672a3a6ca7b4d96b0e09a86dee42e2c3` |
| Grounding DINO base | `12bdfa3120f3e7ec7b434d90674b3396eccf88eb` | `5548f844c928c4b6f411fa8cbcc2bfa8dbbba437cb1d513975519f93c2a9ed21` |

Florenceの公式実装は旧KVキャッシュ形式のため、比較では `use_cache=False` を明示する。重みを変えず速度の代償を受け入れる。SAM2の比較は設定に一致する `Sam2VideoModel` の単一フレーム経路を使い、重みキーの不一致を拒否する。内部APIへの依存は固定Transformers版限定であり、製品採用時には正規アダプターとテストが必要。結果は部位候補で、独立した上下唇・瞳・白目・隠れ領域の完成や動作合格を意味しない。

### 通常の開発環境

| 項目 | 版・備考 |
|---|---|
| Rust（cargo） | 安定版 latest |
| Tauri CLI | `cargo install tauri-cli --version '^2'` |
| WebView2 ランタイム | Windows のみ。多くの環境で導入済み |
| **CUDA 対応 NVIDIA GPU** | **必須。VRAM 8GB 以上。** CPU フォールバックは実装しない |
| uv | Python 3.12.13の開発用単一環境を構築するために使用。製品利用者には要求しない |

**Node.js はアプリとビルドの依存にしない。** 作業用ツール（スクリーンショット撮影など）としての利用は可。判断基準は「アプリのビルド・起動・配布に Node が要るようになるか」（[AGENTS.md](../AGENTS.md)）。

**Python は利用者側では不要。** ランタイムを同梱する。ただし**同梱する Python 環境は1つだけ**で、ComfyUI・画像→3D・リギングが同じ Python 3.12 環境を共有する（[SPEC.md](../SPEC.md) 2 方針5）。**用途ごとに環境を分けない。** 開発時は `cargo xtask setup sidecar` が用意する。

依存が衝突したら環境を増やすのではなく版を揃えて解決する。解決できない場合だけ理由を明記して利用者へ相談する。

## ComfyUI（同梱）

画像生成・編集のバックエンドは ComfyUI（[SPEC.md](../SPEC.md) 4.7.2）。開発時も**同梱版を使い、開発機の手元 ComfyUI に依存しない**。手元環境で動いて利用者環境で動かない、が最も起きやすい失敗。

- ComfyUI は **v0.34.0** を固定する。GPL-3.0 の本文、著作権表示、対応ソースの提供方法を配布物へ含める
- `cargo xtask setup comfy` が固定タグを取得し、コミットIDまで照合する。初期構成は標準ノードだけを使う
- カスタムノードは現時点では同梱しない。追加する場合は固定コミット、ライセンス、重み、直接・推移Python依存を監査し、自動更新しない
- ワークフロー JSON はリポジトリで管理する
- ポートは利用者の既存 ComfyUI（既定 8188）と衝突させない
- **常駐 ComfyUI と単発の画像→3D生成を同時に走らせない。** VRAM を取り合う（[SPEC.md](../SPEC.md) 3.1）

### T0 ライセンス監査で固定した取得元

下表のリビジョンとSHA-256はライセンス監査時点の固定値。ファイル本体はリポジトリへ入れず、セットアップ実装時に取得してSHA-256を照合する。候補を更新する場合はライセンス監査もやり直す。

| 対象 | 固定版・リビジョン | ファイルとSHA-256 |
|---|---|---|
| ComfyUI | `v0.34.0` | Gitタグを固定。Python 3.12/CUDA/PyTorchを含む全依存はT1/T2でlockfile化して別途ハッシュを固定 |
| rembg | `v2.0.83` | パッケージのwheelハッシュはT5のlockfileで固定 |
| isnet-anime | `skytnt/anime-seg@493cb60893f47441b26ec4fb9a306bce9e342982` | `isnetis.onnx`: `f15622d853e8260172812b657053460e20806f04b9e05147d49af7bed31a6e99` |
| SAM 2.1 Hiera Tiny | `facebook/sam2.1-hiera-tiny@de431c4043854a71d8101e17995dfe596bf101a5` | `model.safetensors`: `48c14467e5cf9e51870511feb72c89688e82dd74523142c0538b663e193ac2a7`。設定3ファイルも個別にSHA-256固定 |
| TripoSR | コード `107cefdc244c39106fa830359024f6a2f1c78871`、重み `stabilityai/TripoSR@5b521936b01fbe1890f6f9baed0254ab6351c04a` | `model.ckpt`: `429e2c6b22a0923967459de24d67f05962b235f79cde6b032aa7ed2ffcd970ee` |
| DINO ViT-B/16設定 | `facebook/dino-vitb16@f205d5d8e640a89a2b8ef0369670dfc37cc07fc2` | `config.json`: `b87c0270b97db085fd82cf114a761fd0f62ae7914fbd407c752a2260646b689c`。重みはTripoSR checkpoint内 |
| TRELLIS（条件付き代替） | コード `442aa1e1afb9014e80681d3bf604e8d728a86ee7`、重み `microsoft/TRELLIS-image-large@25e0d31ffbebe4b5a97464dd851910efc3002d96` | 複数ファイル構成。採用時に全manifestを固定し、`diffoctreerast` は取得しない |
| Animagine XL 4.0 Opt | `cagliostrolab/animagine-xl-4.0@2b7c1b397761bf5bd3cc42e5b39ec99314a75a96` | `animagine-xl-4.0-opt.safetensors`: `6327eca98bfb6538dd7a4edce22484a1bbc57a8cff6b11d075d40da1afb847ac` |
| ControlNet Canny SDXL | `diffusers/controlnet-canny-sdxl-1.0@eb115a19a10d14909256db740ed109532ab1483c` | `diffusion_pytorch_model.safetensors`: `ea99040544a999f814fd854575a3aee069a005d026864c8d321b82576706a221` |
| IP-Adapter SDXL Plus Face | `h94/IP-Adapter@018e402774aeeddd60609b4ecdb7e298259dc729` | adapter: `677ad8860204f7d0bfba12d29e6c31ded9beefdf3e4bbd102518357d31a292c1`、image encoder: `657723e09f46a7c3957df651601029f66b1748afb12b419816330f16ed45d64d` |
| Qwen-Image-Edit | `Qwen/Qwen-Image-Edit@ac7f9318f633fc4b5778c59367c8128225f1e3de` | 複数ファイル構成。T2で採用する場合だけ全manifestを固定 |
| llama.cpp + Qwen2.5 1.5B | llama.cpp `v0.3.0`、`Qwen/Qwen2.5-1.5B-Instruct-GGUF@91cad51170dc346986eccefdc2dd33a9da36ead9` | `qwen2.5-1.5b-instruct-q4_k_m.gguf`: `6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e` |
| whisper.cpp + Whisper small | whisper.cpp `b4938`、`ggerganov/whisper.cpp@5359861c739e955e79d9a303bcbc70fb988958b1` | `ggml-small.bin`: `1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b` |

PowerShell を使う場合は **PowerShell 7 の `pwsh`** を既定にする。見つからなければ Windows PowerShell 5.1 へ黙って降格せず、導入が必要な理由を利用者へ伝える。

## リポジトリ構成

構成の詳細と各層の責務は [SPEC.md](../SPEC.md) 4.1 を参照。

```
src-tauri/   Rust本体（設定・パイプライン・フェイスパッチ・リップシンク・配信・サイドカー制御）
ui/          操作UI（webview）。ui/shared/ に three.js 描画コードを置く
ui-stream/   旧OBS試作の説明（現行対象外。現行確認画面はui/check.html）
sidecar/     Python 3.12（分解・2.5Dリグ・局所補完。旧3D試作も保持）
docs/        ドキュメント
xtask/       開発タスク
temp/        一時作成物のみ。.gitignore 済み
```

## コマンド

| コマンド | 用途 |
|---|---|
| `cargo xtask setup engines` | llama.cpp b10621 / whisper.cpp b4938 とQwen・Whisperモデルを取得・SHA-256検証。固定版が揃っていれば再取得しない |
| `cargo xtask setup comfy` | 同梱 ComfyUI 本体・ワークフローと、監査済みの場合だけカスタムノード固定版を用意 |
| `cargo xtask setup sidecar` | Python 3.12 ランタイムと依存を用意（**CUDA wheel は対応GPU検出時のみ**） |
| `cargo xtask setup models` | モデルを取得 |
| `cargo xtask setup completion` | 採用済みQwen局所補完の3モデルだけを取得し固定SHAを検証。Layeredは取得しない |
| `cargo xtask setup sam2` | SAM 2.1 Hiera Tinyだけを固定リビジョンから取得し、全4ファイルのSHA-256を検証 |
| `cargo xtask dev` | 開発起動 |
| `cargo xtask build` | 配布ビルド |
| `cargo xtask verify` | 書式・静的解析・テスト・文書同期をまとめて実行 |
| `cargo xtask facepatch --model <vrm/glb> --neutral <png> --layered-expression-dir <11枚のディレクトリ> --atlas <png> --frame <json> --output-dir <dir> --diagnostics <dir>` | 目・眉N枚と共通口形6枚を合成し、中立スキニング済みメッシュへ逆投影 |
| `cargo xtask expression-import --neutral <png> --input <png> --output <dir> --kind <eyes/mouth> --key <ASCIIキー>` | 外部表情画像の位置・反転・色差を検査し、中立画像と署名を保存 |
| `cargo xtask mesh --input <png> --output <dir>` | anime-segで背景除去し、TripoSRで2048² UVアトラス付きGLBを単発生成 |
| `cargo xtask rig --input <glb> --output <dir> --name <表示名>` | A/Tポーズを検査し、19ボーンとheat diffusionウェイトを持つVRMを単発生成 |
| `cargo run -p local-vtuber-studio --bin lipsync-probe` | 既定マイクを3秒だけ16kHzへ変換し、FFT判定窓を検査して停止 |
| `cargo run -p local-vtuber-studio --bin stream-probe` | 旧OBS試作の記録。現行検証では使わず、ui/check.htmlを使用 |
| `cargo run -p local-vtuber-studio --bin pipeline-probe -- <input.png> <id> <identity-tags>` | 開発用にアプリと同じRustパイプラインをヘッドレス完走 |
| `cargo run -p local-vtuber-studio --bin pipeline-probe -- --only <characterId> <stage>` | 既存キャラの選択工程だけを再実行 |
| `cargo run -p local-vtuber-studio --bin pipeline-probe -- --background <characterId> <backgroundId> "<prompt>"` | アプリと同じComfyUI管理経路でローカル背景を実生成 |
| `cargo run -p local-vtuber-studio --bin engine-probe -- voice <16kHz.wav>` | whisper.cpp→llama.cpp→表情JSON選択を連結確認 |

T2 の表情生成を試す場合は、次の順で一度だけセットアップする。

```powershell
cargo xtask setup comfy
cargo xtask setup sidecar
cargo xtask setup models
cargo xtask expression --input temp/input.png --output temp/expressions --identity-tags "髪・瞳・衣装・アクセサリの英語タグ"
```

`expression` の入力は 1024x1024 RGBA、出力は目・眉5 PNG、任意表情N PNG、共通口形6 PNGおよび `metrics.json`。ComfyUIは `127.0.0.1:58120` のみで起動し、処理後は必ず終了してハンドルを回収する。初回起動の実測が180秒を超えたため、起動待ちは600秒とする。画像生成は denoise 0.65、閉眼だけ0.85を用いる。目と口を重ならない限定マスクへ分離し、逆投影直前に組み合わせる。

背景生成は `workflows/background-txt2img-api.json` の標準ノードだけを使い、1344x756を直接生成する。補間拡大しない。人物を負のプロンプトで除外し、出力は `characters/<id>/backgrounds/<bgId>.png` へ原子的に置換する。

外部画像は1024x1024で、中立キャプチャと同じ画角・向き・背景にする。`expression-import` は中立画像も `neutral.png` として書き出し、位置ずれ12px超、左右反転の可能性、平均色差0.08超を警告する。警告は拒否ではないが、確認せず投影すると破綻し得る。投入画素はSHA-256キャッシュ署名へ含まれる。

`setup sidecar` は `nvidia-smi` でCUDA対応GPUを確認してから、Python 3.12.13とハッシュ固定済み依存を単一環境へ同期する。`setup models` はT0で固定したリビジョンから取得し、SHA-256不一致なら採用せず中間ファイルを削除する。CPUフォールバックはない。

`mesh` は入力原本を `source/input.png` に保存し、`foreground.png`、`reconstruction-input.png`、`texture.png`、`mesh.glb`、`metrics.json` を出力する。全身が画面高の68%未満、腕幅が画面幅の32%未満、中央ずれが12%超ならA/Tポーズ不適合として生成前に停止する。生成中はHugging Faceをオフライン固定し、初回セットアップ以外の外向き通信を許可しない。

`pipeline-probe` も実アプリと同じ設定優先順位を使い、`LVS_PIPELINE_MESH_RESOLUTION` などの環境変数を反映する。工程を再実行すると、その工程以降の状態と依存成果物を無効化する。古いリグ・中立キャプチャ・表情・投影を新しい上流成果物へ混在させない。

TripoSR経路はVRAM・工程接続の技術検証用であり、完成キャラクター用としては品質不適合である。現時点では3D化後の実画面を合格扱いせず、[TASKS.md](TASKS.md) T9の方式決定まで配布品質を主張しない。

`rig` はUVテクスチャ付き単一GLBを読み、正面A/Tポーズでない入力を明示エラーにする。AポーズはVRMのTポーズへ正規化し、自前のグラフheat diffusionで各頂点の上位4ウェイトを決める。出力は `rigged.vrm` と `rig-metrics.json`。同じPython 3.12環境のNumPy・SciPy・trimeshだけを使い、Blenderや追加モデルは同梱しない。

T3/T5 の実表示確認には vendored Three.js 0.185.1（MIT）を使う。`tools/facepatch-view/` をリポジトリルートからローカルHTTP配信し、`model` と、外部テクスチャを確認する場合だけ `texture` のクエリへローカルパスを渡す。投影テクスチャはアンリットで、VRMのglTF UV規約に合わせて外部PNGも `flipY=false` とする。`true` にするとUVアイランドが上下反転し、全身へ別部位が貼られる。製品の描画実装も `ui/shared/vendor/three/` を共有し、CDNへ接続しない。追加モジュール内の `three` 参照も同梱ファイルへの相対参照へ固定し、キャンバスの寸法変更は `ResizeObserver` で投影行列へ反映する。

固定環境は Python 3.12.13、PyTorch 2.11.0+cu128、torchvision 0.26.0+cu128、torchaudio 2.11.0+cu128、Transformers 4.57.6、xatlas 0.0.11。`sidecar/requirements-comfy.lock` はWindows x64向け全推移依存を版とwheelハッシュで固定している。依存を変える場合は `requirements-comfy.in` からlockfileを再生成し、同じGPU実走までやり直す。

`cargo xtask build` は Windows NSIS インストーラを `target/release/bundle/nsis/` へ出力する。署名と自動更新は販売方針決定後の後続タスクとする。

推論エンジン本体とモデルファイルはリポジトリに含めない（`.gitignore` 済み）。初回セットアップとモデル取得は時間がかかり、ネットワークが必要。

## 検証のコスト順序

[AGENTS.md](../AGENTS.md) の通り、安い順に試す。

1. `cargo check` — コンパイル
2. `cargo test --lib` — ロジック
3. `cargo xtask verify` — 目的別PRの最終ローカル検証をまとめるとき
4. **GUI 起動・生成の実走・スクリーンショットは高コストなので最後の手段。** 目視確認が本当に必要なときだけ、複数の確認をまとめて1回で行う

`cargo xtask verify`は通常局所補完・出力トランザクション・確認サーバーのCPUテストも含む。モデル推論やブラウザの目視検査は実行しないため、通過だけで生成品質の合格とはしない。

生成の実走（画像→3D、表情生成）は数分かかり GPU を占有する。パラメータを変えるたびに回さず、**変更が出揃ってから最小回数**だけ実行する。

## プロセスの後始末

保存保護の検証は `sidecar/.venv/Scripts/python.exe -m unittest discover -s sidecar -p test_output_transaction.py`、口の2軸輪郭は作業用Nodeで `node --test tools/test-mouth-geometry.mjs` を使う。いずれもモデル推論不要で、生成例外・公開失敗・中断復旧・二重実行拒否、および全パラメータ範囲の輪郭を検査する。Nodeは製品の起動・ビルド依存にはしない。

- 起動して確認したら、使い終わったプロセスは止める。ロックや再ビルドの無駄を避ける
- **子プロセス（Python サイドカー、推論エンジン）は `kill` して `wait` でハンドルまで回収する。** ゾンビになりやすい
- **プロセスを止める前に、必ず親プロセスとコマンドラインで「自分が起動したもの」かを確かめる。** `python.exe` や `node.exe` は他のアプリも使っているため、名前だけで一括終了すると稼働中のものを壊す

Windows での確認例:

```powershell
pwsh -NoProfile -Command "Get-CimInstance Win32_Process -Filter \"Name='python.exe'\" | Select-Object ProcessId,ParentProcessId,CommandLine"
```

## 依存の更新

- **外部依存は原則として公式 LTS または長期保守安定版**を採用し、無ければ安定版 latest。推論エンジンとモデルは公式の最新安定版へ追従する
- **CUDA を使う Python ランタイムは Python 3.12 に統一する。** GPU・CUDA・PyTorch・CUDA拡張の公式 wheel が同時に対応する組み合わせを選ぶ
- `Cargo.toml` / `Cargo.lock` を変更したら `cargo audit` で既知脆弱性を確認する
- **モデル重みの商用利用条項・地域制限・再配布可否を採用前に確認し、結論を [SPEC.md](../SPEC.md) へ書く**（[AGENTS.md](../AGENTS.md)）

## 配布

Windows 先行で、現時点の配布形式は NSIS。署名・自動更新の要否は販売方針が決まってから判断する（[TASKS.md](TASKS.md) の「後続」）。

配布容量は十進の MB / GB で書く。バイト数の単独表示や併記をしない。
