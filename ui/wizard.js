// wizard.js - 1枚絵からモデルを作るウィザード。
//
// **権利の確認に答えるまで生成を始めない。** 確認を飛ばせる導線を作らないのが、この
// 画面のいちばん大事な仕事。
//
// 進捗はバックエンドからの `pipeline:progress` を受けて出す。失敗もここへ出す
// （ログだけで済ませると、利用者は止まった理由が分からない）。

import { invoke, listen, byId, showError, clearError } from './common.js';

const errorBox = byId('error-message');
const progress = byId('progress');
const progressMessage = byId('progress-message');
let currentJobId = null;

/** 権利の確認が済んでいるときだけ「生成を始める」を押せるようにする。 */
function updateStartButton() {
  const agreed = byId('rights-agreed')?.checked ?? false;
  const notice = byId('license-notice')?.value.trim() ?? '';
  const path = byId('input-path')?.value.trim() ?? '';
  const start = byId('start-generate');
  if (start) {
    start.disabled = !(agreed && notice.length > 0 && path.length > 0);
  }
}

async function buildStageList() {
  const list = byId('stage-list');
  if (!list) return;
  try {
    const stages = await invoke('pipeline_stages');
    list.replaceChildren();
    for (const stage of stages) {
      const item = document.createElement('li');
      item.dataset.stageId = stage.id;
      item.textContent = stage.label;
      if (stage.uses_runtime) {
        const note = document.createElement('span');
        note.className = 'hint';
        note.textContent = 'ランタイムが必要';
        item.append(' ', note);
      }
      list.append(item);
    }
  } catch (error) {
    showError(errorBox, error);
  }
}

function newJobId() {
  // フォルダ名になるので、英数字とハイフンだけにする。
  const stamp = new Date().toISOString().replace(/[^0-9]/g, '').slice(0, 14);
  return `job-${stamp}`;
}

async function startGenerate() {
  clearError(errorBox);
  const path = byId('input-path')?.value.trim() ?? '';
  const notice = byId('license-notice')?.value.trim() ?? '';
  const jobId = newJobId();

  try {
    await invoke('pipeline_create_job', {
      jobId,
      inputPath: path,
      licenseNotice: notice,
    });
    currentJobId = jobId;
    byId('start-generate').disabled = true;
    byId('cancel-generate').disabled = false;
    await invoke('pipeline_run', { jobId });
    if (progressMessage) progressMessage.textContent = '生成が完了しました';
  } catch (error) {
    // 失敗を画面へ出す。工程名と理由がそのまま入っている。
    showError(errorBox, error);
    if (progressMessage) progressMessage.textContent = '生成は完了していません';
  } finally {
    byId('cancel-generate').disabled = true;
    updateStartButton();
  }
}

async function cancelGenerate() {
  if (!currentJobId) return;
  try {
    await invoke('pipeline_cancel', { jobId: currentJobId });
    if (progressMessage) progressMessage.textContent = '中止しています…';
  } catch (error) {
    showError(errorBox, error);
  }
}

async function pickImage() {
  const dialog = globalThis.__TAURI__?.dialog;
  if (!dialog) {
    // ダイアログが使えない環境では、パスの直接入力へ誘導する（黙って何もしない、を避ける）。
    showError(errorBox, new Error('ファイル選択ダイアログを開けません。パスを直接入力してください。'));
    return;
  }
  const selected = await dialog.open({
    multiple: false,
    filters: [{ name: 'イラスト', extensions: ['png', 'webp', 'jpg', 'jpeg'] }],
  });
  if (typeof selected === 'string') {
    byId('input-path').value = selected;
    updateStartButton();
  }
}

async function main() {
  clearError(errorBox);
  await buildStageList();

  byId('rights-agreed')?.addEventListener('change', updateStartButton);
  byId('license-notice')?.addEventListener('input', updateStartButton);
  byId('input-path')?.addEventListener('input', updateStartButton);
  byId('pick-image')?.addEventListener('click', pickImage);
  byId('start-generate')?.addEventListener('click', startGenerate);
  byId('cancel-generate')?.addEventListener('click', cancelGenerate);
  updateStartButton();

  await listen('pipeline:progress', (event) => {
    const payload = event.payload;
    if (!payload || payload.job_id !== currentJobId) return;
    if (progress) progress.value = payload.ratio;
    if (progressMessage) progressMessage.textContent = payload.message;
  });
}

main();
