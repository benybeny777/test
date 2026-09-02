// settings.js - 設定画面。
//
// 入力欄は**コネクタの申告（`config_schema()`）だけ**から組み立てる。ここへ項目を
// 直書きすると、コネクタを足すたびにこの画面も直すことになり、必ずどちらかが遅れる。
//
// 秘密値はバックエンドが画面へ返さない。表示されないのは仕様（開発者ツールやログへ
// 残さないため）。

import { invoke, byId, showError, clearError } from './common.js';

const message = byId('settings-message');
const dirty = byId('dirty');
const changed = new Map();

function markDirty() {
  if (dirty) {
    dirty.textContent = changed.size > 0 ? `未保存の変更: ${changed.size} 件` : '';
  }
}

function createField(field, value) {
  const wrapper = document.createElement('label');
  wrapper.className = 'field';

  const title = document.createElement('span');
  title.className = 'field-label';
  title.textContent = field.label;
  wrapper.append(title);

  let input;
  if (field.kind === 'bool') {
    input = document.createElement('input');
    input.type = 'checkbox';
    input.checked = value === true || value === 'true' || (value === undefined && field.default === 'true');
  } else if (field.kind === 'choice') {
    input = document.createElement('select');
    for (const [choiceValue, choiceLabel] of field.choices) {
      const option = document.createElement('option');
      option.value = choiceValue;
      option.textContent = choiceLabel;
      input.append(option);
    }
    input.value = value ?? field.default;
  } else {
    input = document.createElement('input');
    input.type = field.kind === 'password' ? 'password' : field.kind === 'number' ? 'number' : 'text';
    input.value = value ?? '';
    input.placeholder = field.default ? `既定: ${field.default}` : '';
  }
  input.dataset.key = field.key;
  input.addEventListener('input', () => {
    changed.set(field.key, input.type === 'checkbox' ? input.checked : input.value);
    markDirty();
  });
  input.addEventListener('change', () => {
    changed.set(field.key, input.type === 'checkbox' ? input.checked : input.value);
    markDirty();
  });
  wrapper.append(input);

  const help = document.createElement('span');
  help.className = 'hint';
  help.textContent = field.help;
  wrapper.append(help);

  const key = document.createElement('code');
  key.className = 'field-key';
  key.textContent = field.key;
  wrapper.append(key);

  return wrapper;
}

async function load() {
  clearError(message);
  const schema = await invoke('settings_schema');
  const form = byId('fields');
  form.replaceChildren();
  for (const field of schema.fields) {
    form.append(createField(field, schema.values[field.key]));
  }
  changed.clear();
  markDirty();
}

async function save() {
  clearError(message);
  const patch = {};
  for (const [key, value] of changed) {
    patch[key] = typeof value === 'boolean' ? String(value) : value;
  }
  try {
    await invoke('settings_save', { patch });
    changed.clear();
    markDirty();
    if (message) {
      // 再起動不要であることを伝える（各所が cfg.get() で都度読むため）。
      message.textContent = '保存しました。再起動なしで反映されます。';
      message.hidden = false;
    }
  } catch (error) {
    showError(message, error);
  }
}

async function main() {
  byId('save')?.addEventListener('click', save);
  try {
    await load();
  } catch (error) {
    showError(message, error);
  }
}

main();
