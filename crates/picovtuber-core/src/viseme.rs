// viseme.rs - 口形6種の定義と、リップシンクが出す口の状態。
//
// 口形は「あ・い・う・え・お・ん」の6種で固定する。VRM 1.0 の標準表現名
// （`aa` `ih` `ou` `ee` `oh` `neutral`）へ1対1で対応させ、名前の対応表を
// 実装のあちこちへ写さないためにここを唯一の定義元にする。

/// 口形6種。`n` は「閉じ」で、無音のときの既定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Viseme {
    A,
    I,
    U,
    E,
    O,
    N,
}

impl Viseme {
    /// 全6種。工程の生成漏れ検査と設定画面の並びが同じ順序を使う。
    pub const ALL: [Viseme; 6] = [
        Viseme::A,
        Viseme::I,
        Viseme::U,
        Viseme::E,
        Viseme::O,
        Viseme::N,
    ];

    /// 成果物のファイル名・設定値に使う短い識別子。
    pub fn id(self) -> &'static str {
        match self {
            Viseme::A => "a",
            Viseme::I => "i",
            Viseme::U => "u",
            Viseme::E => "e",
            Viseme::O => "o",
            Viseme::N => "n",
        }
    }

    /// VRM 1.0 の標準表現名。`pack` 工程はこの名前で morph target を対応付ける。
    pub fn vrm_name(self) -> &'static str {
        match self {
            Viseme::A => "aa",
            Viseme::I => "ih",
            Viseme::U => "ou",
            Viseme::E => "ee",
            Viseme::O => "oh",
            Viseme::N => "neutral",
        }
    }

    /// 画面表示用の日本語名。
    pub fn label(self) -> &'static str {
        match self {
            Viseme::A => "あ",
            Viseme::I => "い",
            Viseme::U => "う",
            Viseme::E => "え",
            Viseme::O => "お",
            Viseme::N => "ん（閉じ）",
        }
    }

    /// 識別子から引く。未知の値は `None`（既定へ黙って落とさない）。
    pub fn from_id(id: &str) -> Option<Viseme> {
        Viseme::ALL.into_iter().find(|viseme| viseme.id() == id)
    }
}

/// リップシンクが1フレームごとに出す口の状態。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouthState {
    /// どの口形か。
    pub viseme: Viseme,
    /// 口の開き量 `0.0..=1.0`。`viseme` が `N` のときは 0 に近い。
    pub openness: f32,
}

impl Default for MouthState {
    fn default() -> Self {
        MouthState {
            viseme: Viseme::N,
            openness: 0.0,
        }
    }
}

impl MouthState {
    /// 値域を `0.0..=1.0` へ収めて作る。
    ///
    /// 生成元（マイクの音量・正規化係数）は設定で変えられるため、上限を超えた値が
    /// そのまま描画側へ渡ると morph target が破綻する。入口で必ず収める。
    pub fn new(viseme: Viseme, openness: f32) -> Self {
        MouthState {
            viseme,
            openness: clamp01(openness),
        }
    }
}

/// 値を `0.0..=1.0` へ収める。NaN は 0 とみなす（比較が常に false になり素通りするため）。
pub fn clamp01(value: f32) -> f32 {
    if value.is_nan() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

/// 指数移動平均で平滑化する。`factor` は「新しい値をどれだけ効かせるか」`0.0..=1.0`。
///
/// 生の判定をそのまま描画へ流すと口がガタつく。`factor` が 1.0 なら平滑化なし。
pub fn smooth(previous: f32, current: f32, factor: f32) -> f32 {
    let factor = clamp01(factor);
    clamp01(previous + (current - previous) * factor)
}

#[cfg(test)]
mod tests {
    use super::{clamp01, smooth, MouthState, Viseme};

    #[test]
    fn 口形の識別子と表現名が一意である() {
        let mut ids: Vec<&str> = Viseme::ALL.iter().map(|v| v.id()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "口形の識別子が重複している");

        let mut names: Vec<&str> = Viseme::ALL.iter().map(|v| v.vrm_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "VRM表現名が重複している");
    }

    #[test]
    fn 識別子から引けて未知の値は落とさない() {
        assert_eq!(Viseme::from_id("a"), Some(Viseme::A));
        assert_eq!(Viseme::from_id("n"), Some(Viseme::N));
        // 未知の識別子を既定へ黙って落とすと、設定の書き間違いに気づけない。
        assert_eq!(Viseme::from_id("x"), None);
        assert_eq!(Viseme::from_id(""), None);
    }

    #[test]
    fn 開き量は値域へ収まる() {
        assert_eq!(MouthState::new(Viseme::A, 2.0).openness, 1.0);
        assert_eq!(MouthState::new(Viseme::A, -1.0).openness, 0.0);
        assert_eq!(MouthState::new(Viseme::A, f32::NAN).openness, 0.0);
        assert_eq!(MouthState::default().viseme, Viseme::N);
    }

    #[test]
    fn 平滑化は徐々に近づき値域を超えない() {
        // factor=1.0 は平滑化なし。
        assert_eq!(smooth(0.0, 1.0, 1.0), 1.0);
        // factor=0.0 は前の値のまま。
        assert_eq!(smooth(0.3, 1.0, 0.0), 0.3);
        // 途中は間の値になる。
        let half = smooth(0.0, 1.0, 0.5);
        assert!(half > 0.0 && half < 1.0);
        // 設定値が範囲外でも破綻しない。
        assert_eq!(smooth(0.0, 1.0, 5.0), 1.0);
        assert_eq!(clamp01(f32::INFINITY), 1.0);
    }
}
