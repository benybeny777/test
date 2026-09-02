// field.rs - コネクタが申告する設定項目の型。
//
// コネクタは自分が使う設定を `config_schema()` で申告し、設定画面はその申告だけを見て
// 入力欄を組み立てる。だからコネクタを足しても設定画面のコードを触らずに済む。
//
// 値の正本は `config.rs` の永続設定で、ここにあるのは「どう見せて、どう検証するか」だけ。

use serde::Serialize;

/// 入力欄の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Number,
    Bool,
    /// 秘密値。**平文で保存しない**（`secret.rs` がOSの資格情報保護へ預ける）。
    Password,
    /// 決められた候補から選ぶ。候補は `choices` に入れる。
    Choice,
    /// フォルダ選択。
    Directory,
}

/// 設定項目1件の申告。
#[derive(Debug, Clone, Serialize)]
pub struct Field {
    /// 設定キー（`PICOVTUBER_` 接頭辞）。`docs/SETTINGS.md` に必ず載せること。
    pub key: &'static str,
    /// 設定画面に出す日本語のラベル。
    pub label: &'static str,
    /// 何のための設定かの一言。利用者が読む文章として書く。
    pub help: &'static str,
    pub kind: FieldKind,
    /// 未設定時の既定値。実装側の `cfg.get(key, default)` と同じ値にすること。
    pub default: &'static str,
    /// `Choice` のときの候補（値, 表示名）。
    pub choices: &'static [(&'static str, &'static str)],
}

impl Field {
    pub const fn text(
        key: &'static str,
        label: &'static str,
        help: &'static str,
        default: &'static str,
    ) -> Field {
        Field {
            key,
            label,
            help,
            kind: FieldKind::Text,
            default,
            choices: &[],
        }
    }

    pub const fn number(
        key: &'static str,
        label: &'static str,
        help: &'static str,
        default: &'static str,
    ) -> Field {
        Field {
            key,
            label,
            help,
            kind: FieldKind::Number,
            default,
            choices: &[],
        }
    }

    pub const fn boolean(
        key: &'static str,
        label: &'static str,
        help: &'static str,
        default: &'static str,
    ) -> Field {
        Field {
            key,
            label,
            help,
            kind: FieldKind::Bool,
            default,
            choices: &[],
        }
    }

    pub const fn choice(
        key: &'static str,
        label: &'static str,
        help: &'static str,
        default: &'static str,
        choices: &'static [(&'static str, &'static str)],
    ) -> Field {
        Field {
            key,
            label,
            help,
            kind: FieldKind::Choice,
            default,
            choices,
        }
    }

    pub const fn directory(
        key: &'static str,
        label: &'static str,
        help: &'static str,
        default: &'static str,
    ) -> Field {
        Field {
            key,
            label,
            help,
            kind: FieldKind::Directory,
            default,
            choices: &[],
        }
    }
}
