// studio.js - 配信ビュー。
//
// three-vrm でモデルを描き、口形と表情を毎フレーム反映する。**口形と表情は別チャンネル**
// として合成する（表情が口の形を持っていても、リップシンクの口形を潰さない）。
//
// three.js / three-vrm は Git 管理外で、`cargo xtask setup viewer` が取得する。
// 未取得のときは黙って静止画にせず、理由と対処を画面へ出す。

import { invoke, listen, byId, showError } from './common.js';

const state = {
  studio: 'unloaded',
  vrm: null,
  renderer: null,
  scene: null,
  camera: null,
  clock: null,
  disposers: [],
};

const badge = byId('state-badge');
const message = byId('stage-message');

const STATE_LABELS = {
  unloaded: 'モデル未読込',
  idle: '待機',
  tracking: '追従中',
  streaming: '配信中',
};

async function refreshState() {
  state.studio = await invoke('studio_state');
  if (badge) badge.textContent = STATE_LABELS[state.studio] ?? state.studio;
}

/** three.js と three-vrm を読み込む。未取得なら理由を返す。 */
async function loadViewerLibraries() {
  try {
    const three = await import('./vendor/three.module.js');
    const gltf = await import('./vendor/GLTFLoader.js');
    const vrm = await import('./vendor/three-vrm.module.js');
    return { three, gltf, vrm };
  } catch (error) {
    throw new Error(
      '配信ビューの描画ライブラリがありません。`cargo xtask setup viewer` ' +
        '（製品版は設定画面の「描画ライブラリを取得」）を実行してください。' +
        `（${error instanceof Error ? error.message : error}）`,
    );
  }
}

/** 確保したGPU資源を解放する。モデル切替のたびに増え続けさせない。 */
function disposeCurrent() {
  for (const dispose of state.disposers.splice(0)) {
    try {
      dispose();
    } catch {
      // 解放の失敗で切替を止めない。次の確保に影響しないよう捨てる。
    }
  }
  state.vrm = null;
}

async function loadModel(path) {
  const { three, gltf: gltfLib, vrm: vrmLib } = await loadViewerLibraries();
  disposeCurrent();

  if (!state.renderer) {
    const canvas = byId('viewport');
    // alpha: true が透過出力の前提。ここを落とすと背景が黒く塗られる。
    state.renderer = new three.WebGLRenderer({ canvas, alpha: true, antialias: true });
    state.renderer.setClearColor(0x000000, 0);
    state.scene = new three.Scene();
    state.camera = new three.PerspectiveCamera(30, 1, 0.1, 20);
    state.camera.position.set(0, 1.3, 2.2);
    state.clock = new three.Clock();
    const light = new three.DirectionalLight(0xffffff, 1.2);
    light.position.set(1, 2, 3);
    state.scene.add(light);
    state.scene.add(new three.AmbientLight(0xffffff, 0.6));
    resize();
    window.addEventListener('resize', resize);
  }

  if (!gltfLib.GLTFLoader || !vrmLib.VRMLoaderPlugin) {
    throw new Error(
      'VRM の読み込み器を初期化できません。`cargo xtask setup viewer` を実行し直して、' +
        '描画ライブラリの版を揃えてください。',
    );
  }
  const gltfLoader = new gltfLib.GLTFLoader();
  gltfLoader.register((parser) => new vrmLib.VRMLoaderPlugin(parser));

  const gltf = await gltfLoader.loadAsync(convertPath(path));
  const model = gltf.userData.vrm;
  if (!model) {
    throw new Error('VRM として読めませんでした（VRM 1.0 のファイルを指定してください）');
  }
  state.scene.add(model.scene);
  state.vrm = model;
  state.disposers.push(() => {
    state.scene.remove(model.scene);
    vrmLib.VRMUtils?.deepDispose?.(model.scene);
  });
  render();
}

function convertPath(path) {
  const convert = globalThis.__TAURI__?.core?.convertFileSrc;
  return convert ? convert(path) : path;
}

function resize() {
  if (!state.renderer || !state.camera) return;
  const width = window.innerWidth;
  const height = window.innerHeight;
  state.renderer.setSize(width, height, false);
  state.camera.aspect = width / Math.max(height, 1);
  state.camera.updateProjectionMatrix();
}

/** 描画ループ。口形と表情を別々に載せる。 */
function render() {
  if (!state.renderer || !state.scene || !state.camera) return;
  const delta = state.clock ? state.clock.getDelta() : 0;
  if (state.vrm) {
    state.vrm.update(delta);
  }
  state.renderer.render(state.scene, state.camera);
  requestAnimationFrame(render);
}

/** 口形を反映する。表情側の重みには触らない。 */
export function applyMouth(mouth) {
  const expressions = state.vrm?.expressionManager;
  if (!expressions || !mouth) return;
  for (const name of ['aa', 'ih', 'ou', 'ee', 'oh']) {
    expressions.setValue(name, name === mouth.vrmName ? mouth.openness : 0);
  }
}

/** 表情を反映する。口形側の重みには触らない。 */
export function applyExpression(pairs) {
  const expressions = state.vrm?.expressionManager;
  if (!expressions) return;
  for (const [name, weight] of pairs) {
    expressions.setValue(name, weight);
  }
}

async function buildExpressionButtons() {
  const container = byId('expression-buttons');
  if (!container) return;
  const presets = await invoke('studio_expressions');
  container.replaceChildren();
  for (const preset of presets) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = preset.label;
    button.dataset.expressionId = preset.id;
    button.addEventListener('click', () => {
      applyExpression([[preset.vrmName, 1]]);
    });
    container.append(button);
  }
}

async function buildOutputList() {
  const list = byId('output-list');
  if (!list) return;
  const outputs = await invoke('studio_outputs');
  list.replaceChildren();
  for (const output of outputs) {
    const item = document.createElement('li');
    const available = output.availability.status === 'available';
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = output.label;
    // 使えない出力は押せないようにし、理由を必ず添える。
    // 押せるのに何も起きない状態が、いちばん原因を追いにくい。
    button.disabled = !available;
    button.addEventListener('click', async () => {
      try {
        await invoke('studio_transition', { next: 'streaming' });
        await refreshState();
      } catch (error) {
        showError(message, error);
      }
    });
    item.append(button);
    if (!available) {
      const reason = document.createElement('span');
      reason.className = 'hint';
      reason.textContent = output.availability.reason;
      item.append(reason);
    }
    list.append(item);
  }
}

async function showAutoExpressionAvailability() {
  const element = byId('auto-expression');
  if (!element) return;
  const availability = await invoke('studio_auto_expression_availability');
  if (availability.status === 'ready') {
    element.textContent = '表情の自動切替: 利用できます';
    return;
  }
  // 未導入をクラウドで代替しないことも、あわせて伝える。
  element.textContent = `表情の自動切替: 利用不可 — ${availability.reason}（${availability.remedy}）`;
}

function wireButtons() {
  byId('start-input')?.addEventListener('click', async () => {
    try {
      await invoke('studio_transition', { next: 'tracking' });
      await refreshState();
    } catch (error) {
      showError(message, error);
    }
  });

  byId('stop-all')?.addEventListener('click', async () => {
    try {
      await invoke('studio_transition', { next: 'idle' });
      await refreshState();
    } catch (error) {
      showError(message, error);
    }
  });

  byId('load-model')?.addEventListener('click', async () => {
    try {
      const schema = await invoke('settings_schema');
      const path = schema.values.PICOVTUBER_MODEL_PATH;
      if (!path) {
        throw new Error('設定の「配信に使うモデル」でVRMのパスを指定してください。');
      }
      await loadModel(path);
      await invoke('studio_transition', { next: 'idle' });
      await refreshState();
      const label = byId('model-path');
      if (label) label.textContent = path;
    } catch (error) {
      showError(message, error);
    }
  });
}

async function main() {
  wireButtons();
  try {
    await refreshState();
    await buildExpressionButtons();
    await buildOutputList();
    await showAutoExpressionAvailability();
  } catch (error) {
    showError(message, error);
  }
  // 生成の進捗は配信ビューにも届く（ウィザードを閉じても進行が分かるように）。
  await listen('pipeline:progress', (event) => {
    if (message && event.payload) {
      message.textContent = `${Math.round(event.payload.ratio * 100)}% ${event.payload.message}`;
      message.hidden = false;
    }
  });
}

main();
