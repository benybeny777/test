// text.rs - 文字列を安全に切り詰めるための共通層。
//
// Rust の文字列は UTF-8 バイト列で、`&s[..120]` のようなバイト位置での切り出しは
// **その位置が文字の途中だと panic する**。日本語は 1 文字 3 バイトなので、固定長で
// 切ると高い確率で境界を外す。ログ・プレビュー・出力の打ち切りは全体を通して
// ここの関数を使うこと（直接のバイト添字による切り出しはアプリ側の `text::guard` が検出する）。

/// 文字数で切り詰め、切った場合だけ末尾へ `…` を付ける。
///
/// 「バイト数」ではなく「文字数」で数えるので、日本語でも見た目どおりに切れる。
pub fn preview(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

/// バイト上限で切り詰める。**文字の途中では切らない**。
///
/// ログの行長やプロトコル上の上限のように「バイト数」で決まる制約に使う。
/// 上限が 1 文字分にも満たない場合は空文字になる（panic はしない）。
pub fn truncate_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    // `end` は必ず文字境界なので添字が panic しない。
    text.split_at(end).0
}

/// ストリーミング受信のバイト列を、文字の途中で壊さずに逐次デコードする。
///
/// HTTP のチャンク境界や子プロセスの標準出力は文字の途中に落ちる。チャンクごとに
/// `String::from_utf8_lossy` すると、**まだ続きが来ていないだけ**の末尾バイトが
/// `\u{FFFD}` に置き換わって確定し、日本語のメッセージに文字化けが混ざる。
/// 未確定のバイトは次の `push` まで持ち越すこと。
#[derive(Default)]
pub struct Utf8Stream {
    /// 文字の途中で終わったぶんの持ち越し。
    pending: Vec<u8>,
}

impl Utf8Stream {
    /// 受け取ったバイト列のうち、**文字として完成した分だけ**を返す。
    pub fn push(&mut self, chunk: &[u8]) -> String {
        self.pending.extend_from_slice(chunk);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    return out;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    // 妥当な部分は確定させる。`valid` は文字境界なので添字は安全。
                    out.push_str(std::str::from_utf8(&self.pending[..valid]).unwrap_or_default());
                    match error.error_len() {
                        // 本当に壊れているバイト: 置換文字にして先へ進む。
                        Some(bad) => {
                            out.push('\u{FFFD}');
                            self.pending.drain(..valid + bad);
                        }
                        // 単に途中で切れているだけ: 続きが来るまで持ち越す。
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{preview, truncate_bytes, Utf8Stream};

    #[test]
    fn 日本語を文字数で切り詰める() {
        assert_eq!(preview("こんにちは", 10), "こんにちは");
        assert_eq!(preview("こんにちは", 3), "こんに…");
        assert_eq!(preview("", 3), "");
    }

    #[test]
    fn バイト上限でも文字の途中で切らない() {
        // 「あ」は 3 バイト。上限 4 バイトなら 1 文字だけ残る（panic しない）。
        assert_eq!(truncate_bytes("あああ", 4), "あ");
        assert_eq!(truncate_bytes("あああ", 9), "あああ");
        assert_eq!(truncate_bytes("あああ", 2), "");
        assert_eq!(truncate_bytes("abc", 2), "ab");
    }

    #[test]
    fn チャンク境界で分断された日本語を化けさせない() {
        let source = "こんにちは、今日はいい天気ですね。";
        let bytes = source.as_bytes();
        // 1 バイトずつ流す＝毎回どこかの文字の途中で切れる、最悪ケース。
        let mut stream = Utf8Stream::default();
        let mut out = String::new();
        for byte in bytes {
            out.push_str(&stream.push(&[*byte]));
        }
        assert_eq!(out, source);
        assert!(!out.contains('\u{FFFD}'));

        // 参考: 同じ切り方を from_utf8_lossy でやると壊れる（これが避けたい不具合）。
        let broken: String = bytes
            .iter()
            .map(|byte| String::from_utf8_lossy(&[*byte]).into_owned())
            .collect();
        assert!(
            broken.contains('\u{FFFD}'),
            "旧実装なら化ける入力であること"
        );
    }

    #[test]
    fn 壊れたバイト列は置換文字にして先へ進む() {
        let mut stream = Utf8Stream::default();
        // 0xFF は UTF-8 として不正。ここで止まらず後続を返せること。
        assert_eq!(stream.push(&[b'a', 0xFF, b'b']), "a\u{FFFD}b");
        // 末尾が文字の途中で終わったストリームは、続きが来るまで何も返さない。
        let mut partial = Utf8Stream::default();
        assert_eq!(partial.push(&[0xE3, 0x81]), "");
        assert_eq!(partial.push(&[0x82]), "あ");
    }

    #[test]
    fn 日本語を含む進捗メッセージのプレビューでpanicしない() {
        // 生成工程の進捗はスクリプトの標準出力から来るため、途中で切れた日本語が混ざる。
        let raw = format!("工程 multiview:{}", "側面ビューを生成中".repeat(20));
        assert!(
            !raw.is_char_boundary(120),
            "この入力は 120 バイト目が文字の途中"
        );
        let shown = preview(&raw, 120);
        assert!(shown.chars().count() <= 121);
        assert!(shown.ends_with('…'));
    }
}
