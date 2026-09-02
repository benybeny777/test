// doc_sync.rs - ドキュメントと実装のズレを検出する設計ガード（テスト専用）。
//
// 設定キーと工程一覧は「実装を直したのに文書を直し忘れる」「文書から消したのに実装に
// 残る」がどちらも起きる。人手で突き合わせる運用は必ず抜けるので、双方向で機械検査する。
//
// ここが落ちたときの直し方は2つだけ。**実装を直すか、文書を直すか。** ガードの例外を
// 増やして通すのは、検査していないのと同じになる。

/// 設定キーの正本。
const SETTINGS_DOC: &str = "docs/SETTINGS.md";
/// 工程分類表の正本。
const SPEC_DOC: &str = "SPEC.md";
/// 工程分類表がある節の見出し。SPEC には表が複数あるので、この節だけを読む。
const STAGE_TABLE_HEADING: &str = "### 2.2 工程分類表";

/// `PICOVTUBER_` で始まるが設定キーではない文字列を、`("キー名", "理由")` で登録する。
///
/// いまは空。テストコードは走査前に落としているので、登録が要るのは**本体コードに
/// 置いた設定キーでない識別子**だけ。ここを増やすほど検査は緩むので、まず「本当に
/// 設定キーではないか」を確かめること。
const NOT_CONFIG_KEYS: [(&str, &str); 0] = [];

/// ソースから設定キーらしき文字列を集める。
fn keys_in_sources() -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for source in crate::guard_scan::all_sources() {
        if source.name == "doc_sync.rs" {
            continue; // 走査の定義元（このファイル自身の説明文を含む）
        }
        for key in extract_keys(source.body()) {
            out.insert(key);
        }
    }
    out
}

/// 文字列から `PICOVTUBER_...` を抜き出す。
fn extract_keys(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("PICOVTUBER_") {
        let tail = rest.split_at(start).1;
        let key: String = tail
            .chars()
            .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        // 接頭辞だけ（`PICOVTUBER_` で終わる説明文）は設定キーではない。
        if key.len() > "PICOVTUBER_".len() && !key.ends_with('_') {
            out.push(key.clone());
        }
        rest = tail.split_at(key.len().max(1)).1;
    }
    out
}

/// `docs/SETTINGS.md` に載っているキー。
fn keys_in_doc() -> std::collections::BTreeSet<String> {
    let path = crate::guard_scan::repo_root().join(SETTINGS_DOC);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{SETTINGS_DOC} を読めません: {error}"));
    extract_keys(&text).into_iter().collect()
}

/// `SPEC.md` の工程分類表に載っている工程ID。
fn stages_in_spec() -> std::collections::BTreeSet<String> {
    let path = crate::guard_scan::repo_root().join(SPEC_DOC);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{SPEC_DOC} を読めません: {error}"));
    let mut out = std::collections::BTreeSet::new();
    // SPEC には表が複数あるので、**工程分類表の節だけ**を見る。節を絞らないと、
    // 実行文脈の項目表や配信出力の表まで工程IDとして拾ってしまう。
    let section = text
        .split_once(STAGE_TABLE_HEADING)
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("{SPEC_DOC} に「{STAGE_TABLE_HEADING}」の節がありません"));
    let section = section
        .split_once("\n## ")
        .map(|(head, _)| head)
        .unwrap_or(section);
    let section = section
        .split_once("\n### ")
        .map(|(head, _)| head)
        .unwrap_or(section);

    // 分類表の行は `| \`工程ID\` | …` の形。
    for line in section.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let Some(first) = line.split('|').nth(1) else {
            continue;
        };
        let first = first.trim();
        let Some(id) = first
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
        else {
            continue;
        };
        if id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && !id.is_empty()
        {
            out.insert(id.to_string());
        }
    }
    out
}

#[cfg(test)]
mod guard {
    use super::{keys_in_doc, keys_in_sources, stages_in_spec, NOT_CONFIG_KEYS, SETTINGS_DOC};
    use crate::guard_scan;

    /// 実装が読む設定キーが、全部 `docs/SETTINGS.md` に載っていること。
    ///
    /// 載っていないキーは、利用者から見て「存在しない設定」になる。
    #[test]
    fn 実装の設定キーは全部一覧に載っている() {
        let documented = keys_in_doc();
        let missing: Vec<String> = keys_in_sources()
            .into_iter()
            .filter(|key| !documented.contains(key))
            .filter(|key| !NOT_CONFIG_KEYS.iter().any(|(excluded, _)| excluded == key))
            .collect();
        assert!(
            missing.is_empty(),
            "{SETTINGS_DOC} に載っていない設定キーがあります（実装を足したら一覧も同じ作業で更新してください）: {missing:?}"
        );
    }

    /// 一覧にだけ残ったキーが無いこと。
    ///
    /// 実装から消したキーが一覧に残ると、利用者は効かない設定を触り続ける。
    #[test]
    fn 一覧だけに残った設定キーが無い() {
        let implemented = keys_in_sources();
        let stale: Vec<String> = keys_in_doc()
            .into_iter()
            .filter(|key| !implemented.contains(key))
            .filter(|key| !NOT_CONFIG_KEYS.iter().any(|(excluded, _)| excluded == key))
            .collect();
        assert!(
            stale.is_empty(),
            "実装が参照していない設定キーが {SETTINGS_DOC} に残っています（削除漏れ）: {stale:?}"
        );
    }

    /// 登録済みの工程が、全部 `SPEC.md` の工程分類表に載っていること。
    #[test]
    fn 工程は全部分類表に載っている() {
        let documented = stages_in_spec();
        let missing: Vec<&'static str> = crate::pipeline::all_stages()
            .iter()
            .map(|stage| stage.id())
            .filter(|id| !documented.contains(*id))
            .collect();
        assert!(
            missing.is_empty(),
            "SPEC.md の工程分類表に載っていない工程があります: {missing:?}"
        );
    }

    /// 分類表にだけ残った工程が無いこと。
    #[test]
    fn 分類表だけに残った工程が無い() {
        let implemented: Vec<&'static str> = crate::pipeline::all_stages()
            .iter()
            .map(|stage| stage.id())
            .collect();
        let stale: Vec<String> = stages_in_spec()
            .into_iter()
            .filter(|id| !implemented.contains(&id.as_str()))
            .collect();
        assert!(
            stale.is_empty(),
            "実装に無い工程が SPEC.md の分類表に残っています（削除漏れ）: {stale:?}"
        );
    }

    /// 抜き出しの検出漏れがあると、両方向の照合がまとめて無意味になる。
    #[test]
    fn 設定キーの抜き出しが代表的な書き方を拾う() {
        let cases = [
            (
                "cfg.get(\"PICOVTUBER_LIPSYNC_GAIN\", \"8.0\")",
                "PICOVTUBER_LIPSYNC_GAIN",
            ),
            (
                "| `PICOVTUBER_MODELS_DIR` | 説明 |",
                "PICOVTUBER_MODELS_DIR",
            ),
            (
                "Field::number(\n  \"PICOVTUBER_OUTPUT_FPS\",",
                "PICOVTUBER_OUTPUT_FPS",
            ),
        ];
        for (text, expected) in cases {
            let keys = super::extract_keys(text);
            assert!(
                keys.iter().any(|key| key == expected),
                "{text} から {expected} を拾えない: {keys:?}"
            );
        }
        // 接頭辞だけの説明文を設定キーとして拾わない。
        assert!(super::extract_keys("PICOVTUBER_ で始まるキー").is_empty());
    }

    /// 工程IDの抜き出しが表以外を拾っていないこと。
    #[test]
    fn 工程idの抜き出しが分類表だけを見る() {
        let ids = stages_in_spec();
        assert!(ids.contains("preprocess"), "分類表を読めていない: {ids:?}");
        assert!(
            !ids.contains("requires"),
            "表以外のバッククォートを拾っている: {ids:?}"
        );
    }

    /// 走査範囲は共通のものを使う（自前でディレクトリを歩かない）。
    #[test]
    fn 走査は共通の範囲を使う() {
        assert!(!guard_scan::all_sources().is_empty());
    }
}
