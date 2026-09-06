# 2.5Dキャラクター確認

原画1枚から通常の4工程を実行した成果物です。左は変更しない原画、右は本体と共通の2.5Dレンダラーです。2026年9月7日、Chrome・1440×1050の同じ確認画面で撮影しています。画像をタップすると拡大表示できます。

これは実走・動作の確認用であり、ひより相当の全身可動や全キャラの最終品質合格を示すページではありません。口は現在の形で暫定承認され、追加造形調整を保留しています。

## PicoAgent実写テスト

依頼対象の`photoreal-prototype`（ピンクのショートヘア、黒とピンクの衣装）を使用。別の女性素材ではありません。1024×1536の原画から同じ通常工程で生成し、局所閉眼は608×608の原寸領域を編集しています。原画SHAの一致を検査済みです。

[![原画と通常生成の全体比較](character-gallery-25d/pico-photo/full.png)](character-gallery-25d/pico-photo/full.png)

| 中立 | 半閉眼 | 完全閉眼 |
|---|---|---|
| [![中立](character-gallery-25d/pico-photo/neutral.png)](character-gallery-25d/pico-photo/neutral.png) | [![半閉眼](character-gallery-25d/pico-photo/half.png)](character-gallery-25d/pico-photo/half.png) | [![完全閉眼](character-gallery-25d/pico-photo/closed.png)](character-gallery-25d/pico-photo/closed.png) |

[開口「あ」](character-gallery-25d/pico-photo/mouth-a.png)・[小角度の顔向き](character-gallery-25d/pico-photo/left.png)

確認範囲: 通常4工程の完走、原画保持、中立・半閉眼・完全閉眼・開口・小角度表示。眉下の細い境界と、実写に対する口内のイラスト寄りの質感は残っています。素材充足表示の`incomplete`を完成品質の意味へ読み替えないでください。

## むぎ（通常生成・品質修正中）

2976×4175の原画から、同じ4工程・同じ設定で生成しました。局所編集は原寸832×832です。通常生成の完走と、保存済み生成画像の再利用を確認しました。

[![むぎの全体比較](character-gallery-25d/mugi/full.png)](character-gallery-25d/mugi/full.png)

| 中立 | 半閉眼 | 完全閉眼 |
|---|---|---|
| [![中立](character-gallery-25d/mugi/neutral.png)](character-gallery-25d/mugi/neutral.png) | [![半閉眼](character-gallery-25d/mugi/half.png)](character-gallery-25d/mugi/half.png) | [![完全閉眼](character-gallery-25d/mugi/closed.png)](character-gallery-25d/mugi/closed.png) |

[開口「あ」](character-gallery-25d/mugi/mouth-a.png)

**未合格:** 左目上の黒い残片と目周囲の境界が残っています。生成画像にも存在するため、抽出成功だけを合格扱いしません。耳輪郭の修正はこの通常出力にはまだ入っていません。利用者承認済みの旧「むぎ・耳＋閉眼修正候補」は別に保持しています。

## 女性A

1254×1254の原画から同じ4工程で完走しました。顔周辺は原寸256×256の領域を使い、画像全体を256へ縮小したものではありません。原画SHA一致を検査済みです。

[![女性Aの全体比較](character-gallery-25d/female-a/full.png)](character-gallery-25d/female-a/full.png)

| 半閉眼 | 完全閉眼 |
|---|---|
| [![半閉眼](character-gallery-25d/female-a/half.png)](character-gallery-25d/female-a/half.png) | [![完全閉眼](character-gallery-25d/female-a/closed.png)](character-gallery-25d/female-a/closed.png) |

目の上下に細い境界・残片があるため最終品質は未合格です。顔を大きく表示した際の原画自体の解像感も確認できます。原画を補間拡大した素材へ置き換えてはいません。

## 動かして確認する

開発PCで確認サーバー起動中は、[キャラ選択付き確認画面](http://10.76.3.1:8791/ui/check.html)から切り替えられます。このリンクは同じネットワーク内だけで利用できます。上の画像はGitHubから閲覧でき、開発PCが停止していても参照できます。

「口パク動作テスト（無音）」と「自動まばたき」は別々に開始します。「固定口形」で閉口・母音を静止確認できます。

旧3Dの失敗比較は[過去の3Dギャラリー](LEGACY_3D_GALLERY.md)へ分離しました。現在の2.5Dの完成例ではありません。
