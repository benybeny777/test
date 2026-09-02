// studio/expression.rs - 配信中の表情の決定と、切り替えの補間。
//
// 表情はプリセット（喜／怒／驚／悲／楽）とテキスト指定のどちらからも決まるが、行き先は
// 同じ重みベクトル1本にする。2系統の状態を持つと、片方を変えたときにもう片方が古い値を
// 出し続ける。
//
// **口形とは別チャンネル。** 表情が口の形を持つ場合（大きく笑うなど）でも、
// リップシンクが決めた口形の重みを潰さない。潰すと、笑っている間だけ口が動かなくなる。

use std::time::{Duration, Instant};

use picovtuber_core::expression::{Expression, ExpressionWeights};

use crate::config::Config;

/// テキストから表情を選ぶための語彙。
///
/// **PC内の文字列照合だけで判定する。** クラウドの感情分析へは送らない。
/// 語彙をここ1箇所に置くことで、追加・調整が表情の定義と一緒に読める。
const VOCABULARY: [(Expression, &[&str]); 5] = [
    (
        Expression::Joy,
        &[
            "うれし",
            "嬉し",
            "たのし",
            "楽し",
            "やった",
            "最高",
            "ありがと",
            "笑",
            "好き",
            "うまい",
            "すごい",
        ],
    ),
    (
        Expression::Angry,
        &[
            "おこ",
            "怒",
            "むかつ",
            "ひどい",
            "許せ",
            "ふざけ",
            "最悪",
            "やめて",
        ],
    ),
    (
        Expression::Surprised,
        &["びっくり", "驚", "えっ", "まじ", "うそ", "なんで", "そんな"],
    ),
    (
        Expression::Sorrow,
        &[
            "かなし",
            "悲し",
            "つらい",
            "辛い",
            "ごめん",
            "さみし",
            "寂し",
            "泣",
        ],
    ),
    (
        Expression::Fun,
        &[
            "ゆっくり",
            "のんび",
            "落ち着",
            "まったり",
            "ほっと",
            "だいじょうぶ",
            "大丈夫",
        ],
    ),
];

/// テキストからの推定結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextGuess {
    pub expression: Expression,
    /// どれだけ確からしいか `0.0..=1.0`。語が多く当たるほど高い。
    pub confidence: f32,
}

/// テキストに合う表情を推定する。当たる語が1つも無ければ `None`。
///
/// **無理に当てない。** 確信のない推定で表情を切り替えると、話の内容と関係なく顔が
/// 動き続けることになる。
pub fn guess_from_text(text: &str) -> Option<TextGuess> {
    let mut best: Option<TextGuess> = None;
    for (expression, words) in VOCABULARY {
        let hits = words.iter().filter(|word| text.contains(**word)).count();
        if hits == 0 {
            continue;
        }
        // 3語当たれば確信度1.0。1語でも 0.34 は出す（弱い根拠でも「無い」よりは強い）。
        let confidence = (hits as f32 / 3.0).min(1.0);
        if best.is_none_or(|current| confidence > current.confidence) {
            best = Some(TextGuess {
                expression,
                confidence,
            });
        }
    }
    best
}

/// 表情の切り替えを時間をかけて行うコントローラ。
///
/// 瞬間切替は不自然に見えるうえ、自動切替が短時間に何度も判定を変えたときに顔がちらつく。
pub struct ExpressionController {
    from: ExpressionWeights,
    to: ExpressionWeights,
    started: Instant,
    duration: Duration,
}

impl ExpressionController {
    pub fn new(blend: Duration) -> ExpressionController {
        ExpressionController {
            from: ExpressionWeights::neutral(),
            to: ExpressionWeights::neutral(),
            started: Instant::now(),
            duration: blend,
        }
    }

    /// 設定から補間時間を読んで作る。
    pub fn from_config(cfg: &Config) -> ExpressionController {
        ExpressionController::new(blend_duration(cfg))
    }

    /// 補間時間を差し替える（設定画面での保存を配信中に反映する）。
    pub fn set_blend(&mut self, blend: Duration) {
        self.duration = blend;
    }

    /// 目標の表情を差し替える。いまの見た目を起点にするので、補間中でも飛ばない。
    pub fn set_target(&mut self, target: ExpressionWeights) {
        self.from = self.weights();
        self.to = target;
        self.started = Instant::now();
    }

    /// プリセット1つへ切り替える。
    pub fn set_preset(&mut self, expression: Expression, strength: f32) {
        self.set_target(ExpressionWeights::preset(expression, strength));
    }

    /// 素の顔へ戻す。
    pub fn clear(&mut self) {
        self.set_target(ExpressionWeights::neutral());
    }

    /// いまの重み。
    pub fn weights(&self) -> ExpressionWeights {
        self.weights_after(self.started.elapsed())
    }

    /// 開始から `elapsed` 経過した時点の重み（テストから時間を与えられるようにした本体）。
    pub fn weights_after(&self, elapsed: Duration) -> ExpressionWeights {
        if self.duration.is_zero() {
            return self.to;
        }
        let progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        self.from.blend_towards(&self.to, progress)
    }

    /// 補間が終わったか。
    pub fn settled(&self) -> bool {
        self.started.elapsed() >= self.duration
    }
}

/// 設定から補間時間を読む。0 は「瞬間切替」として認めるが、既定にはしない。
pub fn blend_duration(cfg: &Config) -> Duration {
    Duration::from_millis(u64::from(
        cfg.get_u32("PICOVTUBER_EXPRESSION_BLEND_MS", 250).min(5000),
    ))
}

/// 設定から既定の表情を読む。未知の識別子は素の顔へ落とす。
pub fn default_expression(cfg: &Config) -> Option<Expression> {
    let id = cfg.get("PICOVTUBER_EXPRESSION_DEFAULT", "");
    if id.trim().is_empty() {
        return None;
    }
    Expression::from_id(&id)
}

#[cfg(test)]
mod tests {
    use super::{blend_duration, default_expression, guess_from_text, ExpressionController};
    use crate::config::Config;
    use picovtuber_core::expression::{Expression, ExpressionWeights};
    use std::time::Duration;

    #[test]
    fn 言葉から表情を推定する() {
        assert_eq!(
            guess_from_text("やった、うれしい！").map(|guess| guess.expression),
            Some(Expression::Joy)
        );
        assert_eq!(
            guess_from_text("それはひどい、許せない").map(|guess| guess.expression),
            Some(Expression::Angry)
        );
        assert_eq!(
            guess_from_text("えっ、まじで").map(|guess| guess.expression),
            Some(Expression::Surprised)
        );
        assert_eq!(
            guess_from_text("ごめん、つらい").map(|guess| guess.expression),
            Some(Expression::Sorrow)
        );
        assert_eq!(
            guess_from_text("まったり落ち着いていこう").map(|guess| guess.expression),
            Some(Expression::Fun)
        );
    }

    /// 確信のない推定で表情を切り替えると、話と関係なく顔が動き続ける。
    #[test]
    fn 手がかりが無ければ推定しない() {
        assert_eq!(guess_from_text("今日は水曜日です"), None);
        assert_eq!(guess_from_text(""), None);
    }

    #[test]
    fn 当たる語が多いほど確信度が上がる() {
        let weak = guess_from_text("たのしい").expect("1語で当たる");
        let strong = guess_from_text("たのしい、うれしい、最高").expect("3語で当たる");
        assert!(strong.confidence > weak.confidence);
        assert!(strong.confidence <= 1.0);
        assert!(weak.confidence > 0.0);
    }

    #[test]
    fn 切り替えは間の値を通って目標へ届く() {
        let mut controller = ExpressionController::new(Duration::from_millis(200));
        controller.set_preset(Expression::Joy, 1.0);

        let half = controller.weights_after(Duration::from_millis(100));
        assert!(half.get(Expression::Joy) > 0.0);
        assert!(half.get(Expression::Joy) < 1.0, "瞬間切替になっている");

        let done = controller.weights_after(Duration::from_millis(200));
        assert_eq!(done.get(Expression::Joy), 1.0);
    }

    /// 補間中に別の表情へ変えると、現在の見た目から続きが始まること。
    /// 起点を目標側に取ると、切り替えのたびに顔が一瞬飛ぶ。
    #[test]
    fn 補間中の切り替えで顔が飛ばない() {
        let mut controller = ExpressionController::new(Duration::from_millis(1000));
        controller.set_preset(Expression::Joy, 1.0);
        // まだ補間の途中（経過時間はほぼ0）で別の表情へ。
        controller.set_preset(Expression::Angry, 1.0);
        let just_after = controller.weights_after(Duration::from_millis(0));
        assert!(
            just_after.get(Expression::Joy) < 0.2,
            "切り替え直後に喜びが残りすぎている"
        );
        assert!(
            just_after.get(Expression::Angry) < 0.2,
            "切り替え直後に怒りへ飛んでいる"
        );
    }

    #[test]
    fn 補間時間0は瞬間切替として扱う() {
        let mut controller = ExpressionController::new(Duration::ZERO);
        controller.set_preset(Expression::Surprised, 1.0);
        assert_eq!(
            controller
                .weights_after(Duration::ZERO)
                .get(Expression::Surprised),
            1.0
        );
    }

    #[test]
    fn 素の顔へ戻せる() {
        let mut controller = ExpressionController::new(Duration::from_millis(100));
        controller.set_preset(Expression::Sorrow, 1.0);
        controller.clear();
        assert_eq!(
            controller.weights_after(Duration::from_millis(100)),
            ExpressionWeights::neutral()
        );
    }

    #[test]
    fn 補間時間の設定を読む() {
        let cfg = Config::new();
        assert_eq!(blend_duration(&cfg), Duration::from_millis(250));
        cfg.set("PICOVTUBER_EXPRESSION_BLEND_MS", "600");
        assert_eq!(blend_duration(&cfg), Duration::from_millis(600));
        // 極端な値でも配信が止まるほど長くしない。
        cfg.set("PICOVTUBER_EXPRESSION_BLEND_MS", "999999");
        assert_eq!(blend_duration(&cfg), Duration::from_millis(5000));
    }

    #[test]
    fn 既定の表情は未知の識別子を素の顔へ落とす() {
        let cfg = Config::new();
        assert_eq!(default_expression(&cfg), None);
        cfg.set("PICOVTUBER_EXPRESSION_DEFAULT", "fun");
        assert_eq!(default_expression(&cfg), Some(Expression::Fun));
        cfg.set("PICOVTUBER_EXPRESSION_DEFAULT", "ごきげん");
        assert_eq!(default_expression(&cfg), None);
    }
}
