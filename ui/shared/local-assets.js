// ローカル資産の読み込み専用。外部URLとリダイレクトを拒否する。
export function localAssetUrl(source) {
  const url = new URL(source, location.href);
  if (url.origin !== location.origin || !["blob:", "http:", "https:"].includes(url.protocol)) {
    throw new Error("同一オリジンのローカル資産だけを読み込めます");
  }
  return url;
}

export async function loadLocalJson(source, {signal, cache} = {}) {
  const response = await fetch(localAssetUrl(source), {redirect: "error", signal, cache});
  if (!response.ok) throw new Error("リグを読み込めません");
  return response.json();
}
