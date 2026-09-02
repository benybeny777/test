---
name: run-generation-pipeline
description: イラスト1枚からキャラクターを生成する工程（背景除去・画像→3D・リギング・中立キャプチャ・表情生成・逆投影）を正しい順序で実行し、途中工程だけの再実行、キャッシュ署名の判定、失敗時の停止まで扱う依頼で使う。キャラを作り直す、表情だけ再生成する、投影が破綻したときにも使う。
---

# run-generation-pipeline

手順の正本は [.agents/skills/run-generation-pipeline/SKILL.md](../../../.agents/skills/run-generation-pipeline/SKILL.md) です。**必ず最後まで読んでから作業してください。**

このファイルは Claude Code の自動認識用の入口です。手順をここへ複製しないでください。
