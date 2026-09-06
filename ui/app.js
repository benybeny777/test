import { createAvatarRenderer } from "/shared/avatar-renderer.js?v=native-batch12";

const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;
const $ = (selector) => document.querySelector(selector);
const stages = [
  ["isolate", "1 背景除去"],
  ["decompose", "2 SAM 2.1レイヤー分解"],
  ["rig2d", "3 補完前2.5Dリグ"],
  ["complete", "4 原寸の局所閉眼補完"],
];
let characters = [];
let selected;
let selectedStage = "isolate";
let busy = false;
let previewUrls = [];
let previewGeneration = 0;
let backgroundUrl;
const renderer = createAvatarRenderer($("#avatar"), {onError: error => {
  $("#empty-preview").textContent = "描画に失敗しました。キャラクターを選び直してください。";
  $("#empty-preview").hidden = false;
  log("描画エラー: " + error.message);
}});

function log(message) {
  $("#log").textContent = typeof message === "string" ? message : JSON.stringify(message, null, 2);
}

function requireCharacter() {
  if (!selected) throw new Error("キャラクターを選択してください");
  return selected;
}

function framing() {
  return {
    yaw: Number($("#yaw").value),
    pitch: Number($("#pitch").value),
    scale: Number($("#scale").value),
    offsetX: 0,
    offsetY: 0,
    armPose: {
      leftUpperArm: [0, 0, Number($("#left-arm").value)],
      leftLowerArm: [0, 0, 0],
      rightUpperArm: [0, 0, Number($("#right-arm").value)],
      rightLowerArm: [0, 0, 0],
      head: [0, 0, 0],
    },
  };
}

function setBusy(value) {
  busy = value;
  document.querySelectorAll("button").forEach((button) => {
    button.disabled = value;
  });
}

async function action(operation) {
  if (busy) return;
  setBusy(true);
  try {
    await operation();
  } catch (error) {
    log("エラー: " + error);
  } finally {
    setBusy(false);
  }
}

async function refresh() {
  const config = await invoke("get_config");
  $("#sam-batch").value = config.ai.sam2_points_per_batch;
  $("#grounding-model").value = config.ai.grounding_model;
  $("#grounding-threshold").value = config.ai.grounding_threshold;
  $("#eye-context-margin").value = config.ai.eye_context_margin;
  $("#sam-iou").value = config.ai.sam2_pred_iou_threshold;
  $("#sam-stability").value = config.ai.sam2_stability_threshold;
  for (const key of ["model_dir", "steps", "seed", "resolution", "mask_margin", "timeout_seconds"]) {
    $("#completion-" + key).value = config.ai["completion_" + key];
  }
  $("#completion-fast_disk").checked = config.ai.completion_fast_disk;
  characters = await invoke("list_characters");
  if (selected) {
    selected = characters.find((value) => value.characterId === selected.characterId);
  }
  if (!selected && characters.length) selected = characters[0];
  renderCharacters();
  renderSelected();
}

function renderCharacters() {
  const root = $("#characters");
  root.replaceChildren();
  for (const character of characters) {
    const button = document.createElement("button");
    button.className = "character" + (selected?.characterId === character.characterId ? " selected" : "");
    button.textContent = character.displayName;
    button.addEventListener("click", () => {
      selected = character;
      renderCharacters();
      renderSelected();
      if (previewReady(selected)) {
        action(loadPreview);
      } else {
        renderer.clear();
        $("#empty-preview").textContent = "完成キャラクターを選ぶと2.5Dプレビューを表示します。";
        $("#empty-preview").hidden = false;
      }
    });
    root.append(button);
  }
  if (!characters.length) root.textContent = "まだ登録されていません";
}

function renderSelected() {
  const pipeline = $("#pipeline");
  pipeline.replaceChildren();
  for (const [key, label] of stages) {
    const state = selected?.stages?.[key] ?? { status: "pending", message: "未実行" };
    const button = document.createElement("button");
    button.className = "stage-button" + (selectedStage === key ? " selected" : "");
    button.dataset.status = state.status;
    const strong = document.createElement("strong");
    strong.textContent = label;
    const small = document.createElement("small");
    small.textContent = state.message;
    button.append(strong, small);
    button.addEventListener("click", () => {
      selectedStage = key;
      renderSelected();
    });
    pipeline.append(button);
  }
  const expression = $("#preview-expression");
  expression.replaceChildren();
  const chips = $("#expressions");
  chips.replaceChildren();
  for (const item of selected?.expressions ?? []) {
    expression.add(new Option(item.label, item.key));
    const chip = document.createElement("span");
    chip.className = "chip";
    chip.textContent = item.label + " · " + item.key;
    if (!item.isBuiltIn) {
      const remove = document.createElement("button");
      remove.textContent = "×";
      remove.addEventListener("click", () => action(async () => {
        selected = await invoke("remove_expression", { characterId: selected.characterId, key: item.key });
        await refresh();
      }));
      chip.append(remove);
    }
    chips.append(chip);
  }
  const saved = selected?.framings?.green_screen;
  if (saved) {
    for (const [id, value] of [["yaw", saved.yaw], ["pitch", saved.pitch], ["scale", saved.scale]]) {
      $("#" + id).value = value;
      $("#" + id + "-value").textContent = value;
    }
    $("#left-arm").value = saved.armPose.leftUpperArm[2];
    $("#right-arm").value = saved.armPose.rightUpperArm[2];
  }
}

function bytesUrl(bytes, type) {
  return URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type }));
}

async function loadPreview(expressionKey) {
  const token = ++previewGeneration;
  let pendingUrls = [];
  $("#empty-preview").textContent = "プレビューを読み込み中です。";
  $("#empty-preview").hidden = previewUrls.length > 0;
  const character = requireCharacter();
  try {
  const expression = expressionKey ?? $("#preview-expression").value;
  const mouth = $("#preview-mouth").value;
  const assets = await invoke("load_preview_assets", {
    characterId: character.characterId,
    expressionKey: expression,
    mouthKey: mouth,
  });
  if (token !== previewGeneration) return;
  const config = await invoke("get_config");
  if (token !== previewGeneration) return;
  const rigUrl = bytesUrl(assets.rig, "application/json");
  const partUrls = {};
  pendingUrls = [rigUrl];
  for (const [name, bytes] of Object.entries(assets.parts)) {
    const url = bytesUrl(bytes, "image/png");
    pendingUrls.push(url);
    partUrls[name] = url;
  }
  const applied = await renderer.applyState({
    rigUrl,
    partUrls,
    expressionKey: expression,
    mouthKey: mouth,
    crossfadeMs: config.avatar.crossfade_ms,
    blinkMinMs: config.avatar.blink_min_ms,
    blinkMaxMs: config.avatar.blink_max_ms,
    blinkDurationMs: config.avatar.blink_duration_ms,
    idleSwayDegrees: config.avatar.idle_sway_degrees,
    idleSwayPeriodMs: config.avatar.idle_sway_period_ms,
    ...framing(),
  });
  if (token !== previewGeneration || applied === false) return;
  const previousUrls = previewUrls;
  previewUrls = pendingUrls;
  pendingUrls = [];
  previousUrls.forEach(url => URL.revokeObjectURL(url));
  $("#empty-preview").hidden = true;
  } catch(error) {
    if (token !== previewGeneration) return;
    $("#empty-preview").textContent = "プレビューの読み込みに失敗しました: " + error.message;
    $("#empty-preview").hidden = previewUrls.length > 0;
    throw error;
  } finally {
    pendingUrls.forEach(url => URL.revokeObjectURL(url));
  }
}

$("#create").addEventListener("click", () => action(async () => {
  selected = await invoke("create_character", {
    input: {
      displayName: $("#display-name").value,
      sourcePath: $("#source-path").value,
      identityTags: $("#identity-tags").value,
      personaPrompt: $("#persona").value,
    },
  });
  await refresh();
  log(selected.displayName + "を登録しました。入力原本は上書きしません。");
}));
$("#refresh").addEventListener("click", () => action(refresh));
$("#run-all").addEventListener("click", () => action(async () => {
  const character = requireCharacter();
  log("全工程を開始しました。原寸の局所補完には、このPCの比較実績で約15〜18分かかります。");
  selected = await invoke("run_full_pipeline", { characterId: character.characterId });
  await refresh();
  await loadPreview();
  log("全工程が完了しました");
}));
$("#retry-stage").addEventListener("click", () => action(async () => {
  const character = requireCharacter();
  selected = await invoke("run_pipeline_stage", { characterId: character.characterId, stage: selectedStage });
  await refresh();
  log(selectedStage + "を再実行しました");
}));
$("#load-preview").addEventListener("click", () => action(() => loadPreview()));
$("#save-framing").addEventListener("click", () => action(async () => {
  selected = await invoke("update_framing", { characterId: requireCharacter().characterId, framing: framing() });
  await loadPreview();
  log("構図を保存しました");
}));
$("#generate-background").addEventListener("click", () => action(async () => {
  const result = await invoke("generate_background", {
    characterId: requireCharacter().characterId,
    backgroundId: $("#background-id").value,
    prompt: $("#background-prompt").value,
  });
  if (backgroundUrl) URL.revokeObjectURL(backgroundUrl);
  backgroundUrl = bytesUrl(result.image, "image/png");
  $("#stage").style.background = `center / cover no-repeat url("${backgroundUrl}")`;
  log("背景を保存・表示しました: " + result.path);
}));
$("#add-expression").addEventListener("click", () => action(async () => {
  selected = await invoke("add_expression", {
    characterId: requireCharacter().characterId,
    label: $("#expression-label").value,
    prompt: $("#expression-prompt").value,
  });
  await refresh();
  log("ユーザー表情を追加しました。画像を取り込むか表情工程を再実行してください。");
}));
$("#import-expression").addEventListener("click", () => action(async () => {
  await invoke("import_expression", {
    characterId: requireCharacter().characterId,
    inputPath: $("#import-path").value,
    kind: $("#import-kind").value,
    key: $("#import-key").value,
  });
  log("位置・左右・色差を検査して表情画像を取り込みました");
}));
$("#chat").addEventListener("click", () => action(async () => {
  const result = await invoke("converse", {
    characterId: requireCharacter().characterId,
    input: $("#chat-input").value,
  });
  $("#chat-output").textContent = result.reply;
  $("#preview-expression").value = result.expressionKey;
  await loadPreview(result.expressionKey);
}));
$("#transcribe").addEventListener("click", () => action(async () => {
  const text = await invoke("transcribe", { wavPath: $("#wav-path").value });
  $("#chat-input").value = text;
  log("文字起こし: " + text);
}));
$("#voice-chat").addEventListener("click", () => action(async () => {
  const result = await invoke("voice_chat", {
    characterId: requireCharacter().characterId,
    wavPath: $("#wav-path").value,
  });
  $("#chat-input").value = result.transcript;
  $("#chat-output").textContent = result.reply;
  $("#preview-expression").value = result.expressionKey;
  await loadPreview(result.expressionKey);
  log("音声認識→会話→表情切替をローカルで完了しました");
}));
for (const id of ["yaw", "pitch", "scale", "left-arm", "right-arm"]) {
  $("#" + id).addEventListener("input", () => {
    $("#" + id + "-value").textContent = $("#" + id).value;
  });
}

$("#save-sam-batch").addEventListener("click", () => action(async () => {
  const value = Number($("#sam-batch").value);
  if (!Number.isInteger(value) || value < 1 || value > 64) throw new Error("同時処理点数は1〜64の整数です");
  const config = await invoke("get_config");
  config.ai.sam2_points_per_batch = value;
  const model = $("#grounding-model").value.trim();
  if (!model) throw new Error("意味解析モデルの保存先を指定してください");
  config.ai.grounding_model = model;
  const eyeMargin = Number($("#eye-context-margin").value);
  if (!Number.isFinite(eyeMargin) || eyeMargin < .1 || eyeMargin > 2) throw new Error("目の解析余白は0.1〜2で指定してください");
  config.ai.eye_context_margin = eyeMargin;
  for (const [id,key] of [["grounding-threshold","grounding_threshold"],["sam-iou","sam2_pred_iou_threshold"],["sam-stability","sam2_stability_threshold"]]) {
    const threshold = Number($("#"+id).value);
    if (!Number.isFinite(threshold) || threshold < 0 || threshold > 1) throw new Error("候補閾値は0〜1で指定してください");
    config.ai[key] = threshold;
  }
  await invoke("save_config", {config});
  log("保存しました。次回のレイヤー分解から反映します。");
}));
$("#save-completion").addEventListener("click", () => action(async () => {
  const config = await invoke("get_config");
  const modelDir = $("#completion-model_dir").value.trim();
  if (!modelDir) throw new Error("局所補完モデルの保存先を指定してください");
  config.ai.completion_model_dir = modelDir;
  for (const [key, min, max] of [["steps", 1, 100], ["seed", 0, 4294967295], ["resolution", 256, 4096], ["timeout_seconds", 1, 4294967295]]) {
    const value = Number($("#completion-" + key).value);
    if (!Number.isInteger(value) || value < min || value > max) throw new Error(`局所補完の${key}は${min}〜${max}の整数です`);
    config.ai["completion_" + key] = value;
  }
  if (config.ai.completion_resolution % 16 !== 0) throw new Error("局所補完のROI上限は16の倍数で指定してください");
  const margin = Number($("#completion-mask_margin").value);
  if (!Number.isFinite(margin) || margin <= 0 || margin > .5) throw new Error("局所補完のマスク余白は0超〜0.5で指定してください");
  config.ai.completion_mask_margin = margin;
  config.ai.completion_fast_disk = $("#completion-fast_disk").checked;
  await invoke("save_config", {config});
  log("局所補完設定を保存しました。再起動せず次回の局所補完から反映します。");
}));
const unlistenPipeline = await listen("pipeline-progress", (event) => log(event.payload));
addEventListener("beforeunload", () => {
  unlistenPipeline();
  previewUrls.forEach((url) => URL.revokeObjectURL(url));
  if (backgroundUrl) URL.revokeObjectURL(backgroundUrl);
  renderer.dispose();
});
await action(refresh);
if (previewReady(selected)) {
  await action(loadPreview);
}

function previewReady(character) {
  // フォールバック許可: 補完工程導入前の保存済みキャラは既存リグを保持して表示する。
  return character?.stages?.complete?.status === "complete" ||
    (!character?.model?.rig2d_base && character?.stages?.rig2d?.status === "complete");
}
