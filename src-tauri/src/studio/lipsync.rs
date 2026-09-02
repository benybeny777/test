// studio/lipsync.rs - マイク入力から口形6種を決めるローカルDSP。
//
// **音声はこのPCから出ない。** クラウドの音声認識は使わず、リングバッファ上で解析して
// 使い終えたフレームは捨てる。保存もしない。
//
// 母音の判別は、帯域エネルギーの大小ではなく**第1・第2フォルマントの位置**で行う。
// 日本語の5母音は (F1, F2) の組でよく分かれており、話者の声の高さ（基本周波数）が
// 変わってもこの組はあまり動かない。エネルギー比だけで分けると、声の大きい人と小さい人で
// 判定が入れ替わる。

use picovtuber_core::viseme::{clamp01, smooth, MouthState, Viseme};

use crate::config::Config;

/// 母音ごとの (F1, F2) の目安。単位はHz。
///
/// 日本語話者の平均的な値。ここを1箇所に置くことで、閾値の調整が「どの母音を狙って
/// いるか」と結びついたまま行える。
const PROTOTYPES: [(Viseme, f32, f32); 5] = [
    (Viseme::A, 800.0, 1200.0),
    (Viseme::I, 300.0, 2300.0),
    (Viseme::U, 350.0, 1300.0),
    (Viseme::E, 500.0, 1900.0),
    (Viseme::O, 500.0, 900.0),
];

/// F1 を探す帯域。
const F1_RANGE: (f32, f32) = (250.0, 900.0);
/// F2 を探す帯域。
const F2_RANGE: (f32, f32) = (900.0, 2600.0);
/// 各帯域を何点で走査するか。増やすと精度が上がるが計算量も増える。
const SCAN_STEPS: usize = 24;

/// リップシンクの調整値。設定から都度読む（設定画面の変更を再起動なしに反映するため）。
#[derive(Debug, Clone, Copy)]
pub struct LipSyncSettings {
    /// これを下回る音量は無音とみなし、口を閉じる。
    pub silence_rms: f32,
    /// 音量を口の開き量へ写すときの倍率。
    pub gain: f32,
    /// 平滑化の強さ（新しい値をどれだけ効かせるか）。小さいほど滑らか。
    pub smoothing: f32,
    /// 1フレームの長さ（ミリ秒）。
    pub frame_ms: u32,
}

impl LipSyncSettings {
    pub fn from_config(cfg: &Config) -> LipSyncSettings {
        LipSyncSettings {
            silence_rms: cfg
                .get_f32("PICOVTUBER_LIPSYNC_SILENCE_RMS", 0.012)
                .clamp(0.0, 1.0),
            gain: cfg
                .get_f32("PICOVTUBER_LIPSYNC_GAIN", 8.0)
                .clamp(0.1, 100.0),
            smoothing: clamp01(cfg.get_f32("PICOVTUBER_LIPSYNC_SMOOTHING", 0.35)),
            frame_ms: cfg.get_u32("PICOVTUBER_LIPSYNC_FRAME_MS", 20).clamp(5, 100),
        }
    }
}

impl Default for LipSyncSettings {
    fn default() -> Self {
        LipSyncSettings {
            silence_rms: 0.012,
            gain: 8.0,
            smoothing: 0.35,
            frame_ms: 20,
        }
    }
}

/// 口形の推定器。フレームごとに `analyze` を呼ぶ。
///
/// 平滑化のために前回の状態を持つので、配信1本につき1つを使い回すこと。
pub struct LipSync {
    settings: LipSyncSettings,
    previous: MouthState,
}

impl LipSync {
    pub fn new(settings: LipSyncSettings) -> LipSync {
        LipSync {
            settings,
            previous: MouthState::default(),
        }
    }

    /// 調整値を差し替える（設定画面での保存を配信中に反映する）。
    pub fn update_settings(&mut self, settings: LipSyncSettings) {
        self.settings = settings;
    }

    /// 1フレームぶんの音を口の状態へ変換する。
    ///
    /// `samples` はモノラルの `-1.0..=1.0`。使い終えたら呼び出し側が捨てること
    /// （このメソッドは中身を保持しない）。
    pub fn analyze(&mut self, samples: &[f32], sample_rate: u32) -> MouthState {
        let level = rms(samples);
        let target = if samples.is_empty() || sample_rate == 0 || level < self.settings.silence_rms
        {
            // 無音は閉じ。ここで直前の母音を保つと、話し終えても口が開いたままになる。
            MouthState::new(Viseme::N, 0.0)
        } else {
            let windowed = hann_window(samples);
            let f1 = dominant_frequency(&windowed, sample_rate, F1_RANGE);
            let f2 = dominant_frequency(&windowed, sample_rate, F2_RANGE);
            let viseme = nearest_vowel(f1, f2);
            MouthState::new(viseme, level * self.settings.gain)
        };

        // 口形は最も新しい判定を採り、開き量だけを平滑化する。
        // 開き量まで即座に切り替えると口がガタつき、口形まで平滑化すると母音が遅れる。
        let openness = smooth(
            self.previous.openness,
            target.openness,
            self.settings.smoothing,
        );
        self.previous = MouthState::new(target.viseme, openness);
        self.previous
    }

    /// 現在の状態。
    pub fn current(&self) -> MouthState {
        self.previous
    }

    /// 入力を止めたときに口を閉じる。開いたまま固まらせない。
    pub fn reset(&mut self) {
        self.previous = MouthState::default();
    }

    /// このフレーム長で必要なサンプル数。
    pub fn frame_samples(&self, sample_rate: u32) -> usize {
        (sample_rate as usize * self.settings.frame_ms as usize / 1000).max(1)
    }
}

/// 実効値（音量）。
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples
        .iter()
        .map(|sample| {
            if sample.is_finite() {
                sample * sample
            } else {
                0.0
            }
        })
        .sum();
    (sum / samples.len() as f32).sqrt()
}

/// ハン窓。窓をかけないと、フレームの切れ目が広い帯域の雑音として現れ、
/// フォルマントの位置がずれる。
fn hann_window(samples: &[f32]) -> Vec<f32> {
    let count = samples.len();
    if count < 2 {
        return samples.to_vec();
    }
    samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let ratio = index as f32 / (count - 1) as f32;
            let weight = 0.5 - 0.5 * (std::f32::consts::TAU * ratio).cos();
            sample * weight
        })
        .collect()
}

/// 指定周波数の強さ（Goertzel法）。
///
/// FFT を丸ごと回すより、必要な周波数だけを見るほうが軽い。1フレーム 20ms を
/// 24点×2帯域で走査しても、配信のフレームレートに対して十分間に合う。
fn goertzel(samples: &[f32], sample_rate: u32, frequency: f32) -> f32 {
    if samples.is_empty() || sample_rate == 0 {
        return 0.0;
    }
    let normalized = frequency / sample_rate as f32;
    let coefficient = 2.0 * (std::f32::consts::TAU * normalized).cos();
    let (mut previous, mut previous2) = (0.0_f32, 0.0_f32);
    for sample in samples {
        let current = sample + coefficient * previous - previous2;
        previous2 = previous;
        previous = current;
    }
    (previous * previous + previous2 * previous2 - coefficient * previous * previous2).max(0.0)
}

/// 帯域内で最も強い周波数。対数間隔で走査する（低い側の分解能を確保するため）。
fn dominant_frequency(samples: &[f32], sample_rate: u32, range: (f32, f32)) -> f32 {
    let (low, high) = range;
    // 標本化定理の上限を超えた周波数は見ない（折り返しを拾う）。
    let high = high.min(sample_rate as f32 / 2.0 - 1.0);
    if high <= low {
        return low;
    }
    let mut best = (low, f32::MIN);
    for step in 0..SCAN_STEPS {
        let ratio = step as f32 / (SCAN_STEPS - 1) as f32;
        let frequency = low * (high / low).powf(ratio);
        let power = goertzel(samples, sample_rate, frequency);
        if power > best.1 {
            best = (frequency, power);
        }
    }
    best.0
}

/// (F1, F2) が最も近い母音。距離は対数周波数で測る。
///
/// 対数で測るのは、聴覚上の「近さ」が比で決まるから。線形で測ると、高い周波数の
/// 差だけが効いて F1 の違い（あ と い の差）が無視される。
fn nearest_vowel(f1: f32, f2: f32) -> Viseme {
    let mut best = (Viseme::A, f32::MAX);
    for (viseme, proto_f1, proto_f2) in PROTOTYPES {
        let d1 = (f1.max(1.0).ln() - proto_f1.ln()).powi(2);
        let d2 = (f2.max(1.0).ln() - proto_f2.ln()).powi(2);
        let distance = d1 + d2;
        if distance < best.1 {
            best = (viseme, distance);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::{nearest_vowel, rms, LipSync, LipSyncSettings, PROTOTYPES};
    use crate::config::Config;
    use picovtuber_core::viseme::Viseme;

    const SAMPLE_RATE: u32 = 48_000;

    /// 2つのフォルマントを持つ母音らしい信号を作る。
    fn vowel(f1: f32, f2: f32, amplitude: f32, milliseconds: u32) -> Vec<f32> {
        let count = (SAMPLE_RATE * milliseconds / 1000) as usize;
        (0..count)
            .map(|index| {
                let time = index as f32 / SAMPLE_RATE as f32;
                let first = (std::f32::consts::TAU * f1 * time).sin();
                let second = (std::f32::consts::TAU * f2 * time).sin() * 0.7;
                amplitude * (first + second) / 1.7
            })
            .collect()
    }

    #[test]
    fn 無音では口を閉じる() {
        let mut lipsync = LipSync::new(LipSyncSettings::default());
        let silence = vec![0.0_f32; 960];
        let state = lipsync.analyze(&silence, SAMPLE_RATE);
        assert_eq!(state.viseme, Viseme::N);
        assert_eq!(state.openness, 0.0);
    }

    /// 話し終えたあとも直前の母音を保つと、口が開いたまま固まる。
    #[test]
    fn 話し終えたら口形が閉じへ戻る() {
        let mut lipsync = LipSync::new(LipSyncSettings::default());
        lipsync.analyze(&vowel(800.0, 1200.0, 0.5, 20), SAMPLE_RATE);
        let after = lipsync.analyze(&vec![0.0_f32; 960], SAMPLE_RATE);
        assert_eq!(after.viseme, Viseme::N);
    }

    #[test]
    fn 五母音をフォルマントで見分ける() {
        for (expected, f1, f2) in PROTOTYPES {
            let mut lipsync = LipSync::new(LipSyncSettings::default());
            let state = lipsync.analyze(&vowel(f1, f2, 0.6, 40), SAMPLE_RATE);
            assert_eq!(
                state.viseme, expected,
                "F1={f1} F2={f2} を {expected:?} と判定できない"
            );
        }
    }

    /// 話者ごとにフォルマントは10%程度ずれる。ずれで判定が入れ替わらないこと。
    #[test]
    fn フォルマントが一割ずれても同じ母音になる() {
        for (expected, f1, f2) in PROTOTYPES {
            for shift in [0.92_f32, 1.08] {
                let mut lipsync = LipSync::new(LipSyncSettings::default());
                let state = lipsync.analyze(&vowel(f1 * shift, f2 * shift, 0.6, 40), SAMPLE_RATE);
                assert_eq!(
                    state.viseme, expected,
                    "{expected:?} が {shift} 倍のずれで入れ替わった"
                );
            }
        }
    }

    /// 声の大きい人と小さい人で母音の判定が変わってはいけない。
    #[test]
    fn 音量が変わっても母音は変わらない() {
        for amplitude in [0.15_f32, 0.5, 0.95] {
            let mut lipsync = LipSync::new(LipSyncSettings::default());
            let state = lipsync.analyze(&vowel(300.0, 2300.0, amplitude, 40), SAMPLE_RATE);
            assert_eq!(state.viseme, Viseme::I, "音量 {amplitude} で入れ替わった");
        }
    }

    #[test]
    fn 音量が口の開きへつながり値域を超えない() {
        let mut lipsync = LipSync::new(LipSyncSettings {
            smoothing: 1.0, // 平滑化なしで、その場の値を見る
            ..LipSyncSettings::default()
        });
        let quiet = lipsync.analyze(&vowel(800.0, 1200.0, 0.05, 20), SAMPLE_RATE);
        lipsync.reset();
        let loud = lipsync.analyze(&vowel(800.0, 1200.0, 0.9, 20), SAMPLE_RATE);
        assert!(loud.openness > quiet.openness);
        assert!(loud.openness <= 1.0, "開き量が値域を超えている");
    }

    /// 生の判定をそのまま出すと口がガタつく。
    #[test]
    fn 平滑化で開き量が段階的に近づく() {
        let mut lipsync = LipSync::new(LipSyncSettings {
            smoothing: 0.3,
            ..LipSyncSettings::default()
        });
        let first = lipsync.analyze(&vowel(800.0, 1200.0, 0.9, 20), SAMPLE_RATE);
        let second = lipsync.analyze(&vowel(800.0, 1200.0, 0.9, 20), SAMPLE_RATE);
        assert!(
            second.openness > first.openness,
            "平滑化が効いていれば2フレーム目のほうが大きい"
        );
        assert!(first.openness < 1.0, "1フレームで最大まで飛んでいる");
    }

    #[test]
    fn 設定の異常値でも破綻しない() {
        let cfg = Config::new();
        cfg.set("PICOVTUBER_LIPSYNC_GAIN", "-5");
        cfg.set("PICOVTUBER_LIPSYNC_SMOOTHING", "9");
        cfg.set("PICOVTUBER_LIPSYNC_FRAME_MS", "0");
        cfg.set("PICOVTUBER_LIPSYNC_SILENCE_RMS", "たくさん");
        let settings = LipSyncSettings::from_config(&cfg);
        assert!(settings.gain >= 0.1);
        assert!(settings.smoothing <= 1.0);
        assert!(settings.frame_ms >= 5);
        assert_eq!(settings.silence_rms, 0.012, "読めない値は既定へ");

        let mut lipsync = LipSync::new(settings);
        let state = lipsync.analyze(&vowel(500.0, 900.0, 0.5, 20), SAMPLE_RATE);
        assert!(state.openness.is_finite());
    }

    #[test]
    fn 空の入力やサンプルレート0で落ちない() {
        let mut lipsync = LipSync::new(LipSyncSettings::default());
        assert_eq!(lipsync.analyze(&[], SAMPLE_RATE).viseme, Viseme::N);
        assert_eq!(
            lipsync.analyze(&vowel(800.0, 1200.0, 0.5, 20), 0).viseme,
            Viseme::N
        );
        // 数値でない値が混ざっても音量計算が壊れない。
        assert_eq!(rms(&[f32::NAN, 0.0]), 0.0);
    }

    #[test]
    fn フレーム長からサンプル数を出す() {
        let lipsync = LipSync::new(LipSyncSettings::default());
        assert_eq!(lipsync.frame_samples(48_000), 960);
        assert_eq!(lipsync.frame_samples(16_000), 320);
        // 極端に低いサンプルレートでも0にしない（0だと解析が回らない）。
        assert_eq!(lipsync.frame_samples(10), 1);
    }

    #[test]
    fn 母音の目安が互いに区別できる位置にある() {
        for (viseme, f1, f2) in PROTOTYPES {
            assert_eq!(
                nearest_vowel(f1, f2),
                viseme,
                "{viseme:?} の目安が他の母音に近すぎる"
            );
        }
    }
}
