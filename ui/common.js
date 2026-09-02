// common.js - フロント共通の小道具。
//
// バックエンドの呼び出しと、失敗の見せ方をここへ集約する。各画面で書き分けると、
// 「失敗したのに何も出ない」画面がどこかに1つできる。

const tauri = globalThis.__TAURI__;

/** バックエンドのコマンドを呼ぶ。失敗は投げる（握りつぶさない）。 */
export async function invoke(command, args) {
  if (!tauri) {
    throw new Error('バックエンドに接続できません（ブラウザで直接開いていませんか）');
  }
  return tauri.core.invoke(command, args);
}

/** バックエンドからの通知を受け取る。戻り値は購読を解除する関数。 */
export async function listen(event, handler) {
  if (!tauri) {
    return () => {};
  }
  return tauri.event.listen(event, handler);
}

/**
 * 失敗を画面へ出す。**ログだけで済ませない。**
 * 利用者はコンソールを見ないので、ログに書いただけでは「何も起きなかった」ことになる。
 */
export function showError(element, error) {
  if (!element) return;
  const text = error instanceof Error ? error.message : String(error);
  element.textContent = text;
  element.hidden = false;
}

export function clearError(element) {
  if (!element) return;
  element.textContent = '';
  element.hidden = true;
}

/** 要素を1つ取る。無ければ null（呼び出し側が扱いを決める）。 */
export function byId(id) {
  return document.getElementById(id);
}
