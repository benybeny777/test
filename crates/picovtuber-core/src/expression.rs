// expression.rs - 表情プリセットの定義と、表情の重みベクトル。
//
// プリセットとテキスト指定は同じ重みベクトルへ落とす。VRM 1.0 の標準表現名との
// 対応表をあちこちへ写さないため、ここを唯一の定義元にする。
//
// **口形（`viseme` モジュール）とは別チャンネル**として扱う。表情が口の形を持つ場合
// （大きく笑うなど）でも、口形側の重みを潰さないこと。

use crate::viseme::clamp01;

/// 表情プリセット5種。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Expression {
    Joy,
    Angry,
    Surprised,
    Sorrow,
    Fun,
}

impl Expression {
    /// 全5種。工程の生成漏れ検査と画面の並びが同じ順序を使う。
    pub const ALL: [Expression; 5] = [
        Expression::Joy,
        Expression::Angry,
        Expression::Surprised,
        Expression::Sorrow,
        Expression::Fun,
    ];

    /// 成果物のファイル名・設定値に使う短い識別子。
    pub fn id(self) -> &'static str {
        match self {
            Expression::Joy => "joy",
            Expression::Angry => "angry",
            Expression::Surprised => "surprised",
            Expression::Sorrow => "sorrow",
            Expression::Fun => "fun",
        }
    }

    /// VRM 1.0 の標準表現名。`pack` 工程はこの名前で morph target を対応付ける。
    pub fn vrm_name(self) -> &'static str {
        match self {
            Expression::Joy => "happy",
            Expression::Angry => "angry",
            Expression::Surprised => "surprised",
            Expression::Sorrow => "sad",
            Expression::Fun => "relaxed",
        }
    }

    /// 画面表示用の日本語名。
    pub fn label(self) -> &'static str {
        match self {
            Expression::Joy => "喜",
            Expression::Angry => "怒",
            Expression::Surprised => "驚",
            Expression::Sorrow => "悲",
            Expression::Fun => "楽",
        }
    }

    /// 識別子から引く。未知の値は `None`（既定へ黙って落とさない）。
    pub fn from_id(id: &str) -> Option<Expression> {
        Expression::ALL
            .into_iter()
            .find(|expression| expression.id() == id)
    }
}

/// 表情の重みベクトル。各要素は `0.0..=1.0`。
///
/// 複数の表情を同時に載せられる（驚きながら笑う、など）。合計を 1.0 に正規化しない。
/// 正規化すると、単独の表情を最大まで出したときに他が入るだけで弱まってしまうため。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ExpressionWeights {
    weights: [f32; Expression::ALL.len()],
}

impl ExpressionWeights {
    /// すべて 0（＝素の顔）。
    pub fn neutral() -> Self {
        ExpressionWeights::default()
    }

    /// 1つのプリセットだけを指定の強さで出す。
    pub fn preset(expression: Expression, strength: f32) -> Self {
        let mut out = ExpressionWeights::neutral();
        out.set(expression, strength);
        out
    }

    /// 重みを取り出す。
    pub fn get(&self, expression: Expression) -> f32 {
        self.weights[Self::index(expression)]
    }

    /// 重みを設定する。値域は `0.0..=1.0` へ収める。
    pub fn set(&mut self, expression: Expression, weight: f32) {
        self.weights[Self::index(expression)] = clamp01(weight);
    }

    /// `(VRM表現名, 重み)` の一覧。描画側へ渡す形。
    pub fn to_vrm_pairs(&self) -> Vec<(&'static str, f32)> {
        Expression::ALL
            .into_iter()
            .map(|expression| (expression.vrm_name(), self.get(expression)))
            .collect()
    }

    /// `progress`（`0.0..=1.0`）だけ `target` へ近づけた重みを返す。
    ///
    /// 表情の切り替えは瞬間ではなく時間をかけて行う。瞬間切替は不自然に見えるうえ、
    /// 自動切替が短時間に何度も判定を変えたときに顔がちらつく。
    pub fn blend_towards(&self, target: &ExpressionWeights, progress: f32) -> ExpressionWeights {
        let progress = clamp01(progress);
        let mut out = ExpressionWeights::neutral();
        for expression in Expression::ALL {
            let from = self.get(expression);
            let to = target.get(expression);
            out.set(expression, from + (to - from) * progress);
        }
        out
    }

    fn index(expression: Expression) -> usize {
        Expression::ALL
            .iter()
            .position(|candidate| *candidate == expression)
            .expect("Expression::ALL は全プリセットを含む")
    }
}

#[cfg(test)]
mod tests {
    use super::{Expression, ExpressionWeights};

    #[test]
    fn 表情の識別子と表現名が一意である() {
        let mut ids: Vec<&str> = Expression::ALL.iter().map(|e| e.id()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "表情の識別子が重複している");

        let mut names: Vec<&str> = Expression::ALL.iter().map(|e| e.vrm_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "VRM表現名が重複している");
    }

    #[test]
    fn 識別子から引けて未知の値は落とさない() {
        assert_eq!(Expression::from_id("joy"), Some(Expression::Joy));
        assert_eq!(Expression::from_id("happy"), None, "VRM名では引かせない");
        assert_eq!(Expression::from_id(""), None);
    }

    #[test]
    fn 重みは値域へ収まり合計で正規化しない() {
        let mut weights = ExpressionWeights::neutral();
        weights.set(Expression::Joy, 2.0);
        weights.set(Expression::Surprised, 1.0);
        assert_eq!(weights.get(Expression::Joy), 1.0);
        // 2つ載せても、単独指定した強さが弱まらないこと（正規化していない証明）。
        assert_eq!(weights.get(Expression::Surprised), 1.0);
        assert_eq!(weights.get(Expression::Angry), 0.0);
    }

    #[test]
    fn 切り替えは間の値を通って目標へ届く() {
        let from = ExpressionWeights::preset(Expression::Sorrow, 1.0);
        let to = ExpressionWeights::preset(Expression::Joy, 1.0);

        let half = from.blend_towards(&to, 0.5);
        assert!(half.get(Expression::Joy) > 0.0 && half.get(Expression::Joy) < 1.0);
        assert!(half.get(Expression::Sorrow) > 0.0 && half.get(Expression::Sorrow) < 1.0);

        let done = from.blend_towards(&to, 1.0);
        assert_eq!(done, to);
        let none = from.blend_towards(&to, 0.0);
        assert_eq!(none, from);
    }

    #[test]
    fn vrm名の一覧を全プリセットぶん返す() {
        let pairs = ExpressionWeights::preset(Expression::Fun, 0.5).to_vrm_pairs();
        assert_eq!(pairs.len(), Expression::ALL.len());
        let relaxed = pairs
            .iter()
            .find(|(name, _)| *name == "relaxed")
            .expect("relaxed が含まれる");
        assert_eq!(relaxed.1, 0.5);
    }
}
