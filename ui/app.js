import { createAvatarRenderer } from "/shared/avatar-renderer.js";

const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;
const $ = (selector) => document.querySelector(selector);
const stages = [
  ["mesh", "1 背景除去・3D"],
  ["rig", "2 リギング"],
  ["capture", "3 中立キャプチャ"],
  ["expression", "4 表情生成"],
  ["facepatch", "5 逆投影"],
];
let characters = [];
let selected;
let selectedStage = "mesh";
let busy = false;
let previewUrls = [];
let backgroundUrl;
const renderer = createAvatarRenderer($("#avatar"));

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
      if (selected?.stages?.facepatch?.status === "complete") {
        action(loadPreview);
      } else {
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
  const character = requireCharacter();
  const expression = expressionKey ?? $("#preview-expression").value;
  const mouth = $("#preview-mouth").value;
  const assets = await invoke("load_preview_assets", {
    characterId: character.characterId,
    expressionKey: expression,
    mouthKey: mouth,
  });
  previewUrls.forEach((url) => URL.revokeObjectURL(url));
  previewUrls = [bytesUrl(assets.model, "model/gltf-binary"), bytesUrl(assets.texture, "image/png")];
  if (assets.blinkTexture) previewUrls.push(bytesUrl(assets.blinkTexture, "image/png"));
  await renderer.applyState({
    modelUrl: previewUrls[0],
    textureUrl: previewUrls[1],
    blinkTextureUrl: previewUrls[2],
    expressionKey: expression,
    mouthKey: mouth,
    crossfadeMs: 160,
    blinkMinMs: 2800,
    blinkMaxMs: 6500,
    blinkDurationMs: 140,
    idleSwayDegrees: 0.7,
    idleSwayPeriodMs: 4200,
    ...framing(),
  });
  $("#empty-preview").hidden = true;
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
  log("全工程を開始しました。数分かかります。");
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
$("#start-obs").addEventListener("click", () => action(async () => {
  const url = await invoke("start_obs", {
    characterId: requireCharacter().characterId,
    expressionKey: $("#preview-expression").value,
    mouthKey: $("#preview-mouth").value,
    framing: framing(),
  });
  $("#obs-url").textContent = url + " をOBSブラウザソースへ追加してください";
}));
$("#stop-obs").addEventListener("click", () => action(async () => {
  await invoke("stop_obs");
  $("#obs-url").textContent = "停止しました";
}));

for (const id of ["yaw", "pitch", "scale", "left-arm", "right-arm"]) {
  $("#" + id).addEventListener("input", () => {
    $("#" + id + "-value").textContent = $("#" + id).value;
  });
}

await listen("pipeline-progress", (event) => log(event.payload));
addEventListener("beforeunload", () => {
  previewUrls.forEach((url) => URL.revokeObjectURL(url));
  if (backgroundUrl) URL.revokeObjectURL(backgroundUrl);
  renderer.dispose();
});
await action(refresh);
if (selected?.stages?.facepatch?.status === "complete") {
  await action(loadPreview);
}
