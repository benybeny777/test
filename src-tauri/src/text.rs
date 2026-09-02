// text.rs - 文字列を安全に切り詰めるための共通層（アプリ側の入口）。
//
// 実装は非公開の内部共通crateである picovtuber-core へ置き、アプリ内の参照パスは
// `crate::text::…` に保つ。ここにはアプリ全体を走査する設計ガードだけを置く。

pub use picovtuber_core::text::{preview, truncate_bytes, Utf8Stream};

/// 固定長のバイト添字による文字列の切り出しが残っていないかを走査するガード。
#[cfg(test)]
mod guard {
    use crate::guard_scan;

    /// `[..120]` `[0..2]` `[4..]` のように**数値リテラル**で切り出している箇所を探す。
    /// 変数添字（`&chunk[..n]` など、バイト列の読み込み量で切る用途）は対象外。
    fn fixed_byte_slice(line: &str) -> bool {
        fn numeric_literal(part: &str) -> bool {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_digit() || c == '_')
        }
        let bytes = line.as_bytes();
        for (open, _) in line.match_indices('[') {
            let Some(length) = line.split_at(open + 1).1.find(']') else {
                continue;
            };
            let inner = line.split_at(open + 1).1.split_at(length).0.trim();
            let Some((start, end)) = inner.split_once("..") else {
                continue;
            };
            let end = end.strip_prefix('=').unwrap_or(end).trim();
            let start = start.trim();
            // 両側とも空（`[..]`）や、片側でも変数・式なら対象外。
            if !(numeric_literal(start) || start.is_empty())
                || !(numeric_literal(end) || end.is_empty())
                || (start.is_empty() && end.is_empty())
            {
                continue;
            }
            // 配列リテラルの型注釈（`[u8; 4]`）などと違い、直前が識別子・`)`・`]` の
            // ときだけ「何かを添字で切っている」と判断する。
            let before = bytes
                .split_at(open)
                .0
                .iter()
                .rposition(|byte| !byte.is_ascii_whitespace());
            let Some(before) = before.map(|index| bytes[index] as char) else {
                continue;
            };
            if before.is_alphanumeric() || matches!(before, '_' | ')' | ']' | '"' | '#') {
                return true;
            }
        }
        false
    }

    #[test]
    fn 固定長のバイト添字で文字列を切らない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "text.rs" {
                continue; // 定義元（このガード自身の説明文を含む）
            }
            for (number, line) in source.body().lines().enumerate() {
                // コメント行の説明文（このガード自身の例示を含む）は対象外。
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if fixed_byte_slice(line) {
                    offenders.push(format!("{}:{}", source.path.display(), number + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "文字列は text::preview / text::truncate_bytes で切ってください\
             （バイト添字は文字の途中で panic します）: {offenders:?}"
        );
    }

    #[test]
    fn 固定範囲の検出器は代表的な表記を見落とさない() {
        for source in [
            "&text[..120]",
            "&text[0..2]",
            "&text[4..]",
            "&\"日本語\"[0..3]",
            "method()[1..=2].to_vec()",
        ] {
            assert!(fixed_byte_slice(source), "検出できません: {source}");
        }
        for source in [
            "let bytes = [0_u8; 4];",
            "&chunk[..count]",
            "&items[start..]",
        ] {
            assert!(!fixed_byte_slice(source), "誤検出しました: {source}");
        }
    }

    /// ストリーミング受信は `text::Utf8Stream` を通す。`from_utf8_lossy` を直接使うと、
    /// チャンク境界で分断された文字が `\u{FFFD}` になって確定してしまう。
    #[test]
    fn ストリーミング受信で直接lossy変換しない() {
        let mut offenders = Vec::new();
        for source in guard_scan::all_sources() {
            if source.name == "text.rs" {
                continue; // 定義元
            }
            let body = source.body();
            let streaming = body.contains("bytes_stream()") || body.contains("read_buf(");
            if streaming && body.contains("from_utf8_lossy") {
                offenders.push(source.path.display().to_string());
            }
        }
        assert!(
            offenders.is_empty(),
            "ストリーミング受信は text::Utf8Stream を使ってください\
             （チャンク境界で日本語が化けます）: {offenders:?}"
        );
    }
}
