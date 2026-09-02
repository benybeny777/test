use std::collections::VecDeque;
use std::f32::consts::PI;
use std::sync::mpsc::Sender;

use cpal::{
    SampleFormat, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rustfft::{Fft, FftPlanner, num_complex::Complex32};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouthShape {
    A,
    I,
    U,
    E,
    O,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Formant {
    pub f1: f32,
    pub f2: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LipSyncSettings {
    pub sample_rate: u32,
    pub window_samples: usize,
    pub volume_gate_db: f32,
    pub smoothing_frames: usize,
    pub a_shape_bias: f32,
    pub formants: [Formant; 5],
}

impl Default for LipSyncSettings {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            window_samples: 512,
            volume_gate_db: -40.0,
            smoothing_frames: 4,
            a_shape_bias: 0.9,
            formants: [
                Formant {
                    f1: 775.0,
                    f2: 1175.0,
                },
                Formant {
                    f1: 300.0,
                    f2: 2400.0,
                },
                Formant {
                    f1: 350.0,
                    f2: 1150.0,
                },
                Formant {
                    f1: 475.0,
                    f2: 1950.0,
                },
                Formant {
                    f1: 450.0,
                    f2: 800.0,
                },
            ],
        }
    }
}

pub struct LipSyncAnalyzer {
    settings: LipSyncSettings,
    fft: std::sync::Arc<dyn Fft<f32>>,
    spectrum: Vec<Complex32>,
    history: VecDeque<MouthShape>,
    current: MouthShape,
}

impl LipSyncAnalyzer {
    pub fn new(settings: LipSyncSettings) -> Result<Self, String> {
        if !settings.window_samples.is_power_of_two() || settings.window_samples < 64 {
            return Err("lipsync.window_samples は64以上の2の累乗が必要です".into());
        }
        if settings.sample_rate < 8_000 || settings.smoothing_frames == 0 {
            return Err("リップシンク設定が不正です".into());
        }
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(settings.window_samples);
        let spectrum = vec![Complex32::default(); settings.window_samples];
        Ok(Self {
            settings,
            fft,
            spectrum,
            history: VecDeque::new(),
            current: MouthShape::Close,
        })
    }

    pub fn analyze(&mut self, samples: &[f32]) -> Option<MouthShape> {
        let raw = self.classify(samples);
        self.history.push_back(raw);
        while self.history.len() > self.settings.smoothing_frames {
            self.history.pop_front();
        }
        let smoothed = majority(&self.history);
        if smoothed == self.current {
            None
        } else {
            self.current = smoothed;
            Some(smoothed)
        }
    }

    pub fn classify(&mut self, samples: &[f32]) -> MouthShape {
        if samples.len() < self.settings.window_samples {
            return MouthShape::Close;
        }
        let window = &samples[samples.len() - self.settings.window_samples..];
        let rms =
            (window.iter().map(|sample| sample * sample).sum::<f32>() / window.len() as f32).sqrt();
        let db = 20.0 * rms.max(1.0e-9).log10();
        if db < self.settings.volume_gate_db {
            return MouthShape::Close;
        }

        let denominator = (window.len() - 1) as f32;
        for (index, (source, target)) in window.iter().zip(&mut self.spectrum).enumerate() {
            let hann = 0.5 - 0.5 * (2.0 * PI * index as f32 / denominator).cos();
            *target = Complex32::new(source * hann, 0.0);
        }
        self.fft.process(&mut self.spectrum);
        let magnitudes: Vec<f32> = self.spectrum[..self.settings.window_samples / 2]
            .iter()
            .map(|value| value.norm())
            .collect();
        let envelope = smooth_spectrum(&magnitudes, 3);
        let f1 = peak_frequency(&envelope, self.settings.sample_rate, 250.0, 1000.0);
        let f2 = peak_frequency(
            &envelope,
            self.settings.sample_rate,
            (f1 + 150.0).max(700.0),
            3200.0,
        );
        nearest_vowel(f1, f2, &self.settings)
    }
}

fn smooth_spectrum(values: &[f32], radius: usize) -> Vec<f32> {
    (0..values.len())
        .map(|index| {
            let begin = index.saturating_sub(radius);
            let end = (index + radius + 1).min(values.len());
            values[begin..end].iter().sum::<f32>() / (end - begin) as f32
        })
        .collect()
}

fn peak_frequency(spectrum: &[f32], sample_rate: u32, minimum: f32, maximum: f32) -> f32 {
    let bin_hz = sample_rate as f32 / (spectrum.len() * 2) as f32;
    let begin = (minimum / bin_hz).ceil() as usize;
    let end = ((maximum / bin_hz).floor() as usize).min(spectrum.len().saturating_sub(1));
    (begin..=end)
        .max_by(|left, right| spectrum[*left].total_cmp(&spectrum[*right]))
        .map(|index| index as f32 * bin_hz)
        .unwrap_or(minimum)
}

fn nearest_vowel(f1: f32, f2: f32, settings: &LipSyncSettings) -> MouthShape {
    let shapes = [
        MouthShape::A,
        MouthShape::I,
        MouthShape::U,
        MouthShape::E,
        MouthShape::O,
    ];
    shapes
        .into_iter()
        .zip(settings.formants)
        .enumerate()
        .map(|(index, (shape, formant))| {
            let mut distance = (f1 / formant.f1).ln().powi(2) + (f2 / formant.f2).ln().powi(2);
            if index == 0 {
                distance *= settings.a_shape_bias;
            }
            (shape, distance)
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|result| result.0)
        .unwrap_or(MouthShape::Close)
}

fn majority(history: &VecDeque<MouthShape>) -> MouthShape {
    let shapes = [
        MouthShape::A,
        MouthShape::I,
        MouthShape::U,
        MouthShape::E,
        MouthShape::O,
        MouthShape::Close,
    ];
    shapes
        .into_iter()
        .max_by_key(|shape| {
            history
                .iter()
                .filter(|candidate| *candidate == shape)
                .count()
        })
        .unwrap_or(MouthShape::Close)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VadSettings {
    pub sample_rate: u32,
    pub volume_gate_db: f32,
    pub end_silence_seconds: f32,
    pub min_seconds: f32,
    pub max_seconds: f32,
}

impl Default for VadSettings {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            volume_gate_db: -40.0,
            end_silence_seconds: 0.9,
            min_seconds: 0.5,
            max_seconds: 6.0,
        }
    }
}

pub struct VoiceActivityDetector {
    settings: VadSettings,
    utterance: Vec<f32>,
    silent_samples: usize,
}

pub struct MicrophoneCapture {
    stream: Stream,
    pub device_name: String,
    pub source_sample_rate: u32,
}

#[derive(Debug)]
pub enum MicrophoneEvent {
    Samples(Vec<f32>),
    Error(String),
}

impl MicrophoneCapture {
    pub fn start(
        requested_device: &str,
        target_sample_rate: u32,
        sender: Sender<MicrophoneEvent>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = if requested_device.trim().is_empty() {
            host.default_input_device()
                .ok_or_else(|| "既定のマイクが見つかりません".to_owned())?
        } else {
            host.input_devices()
                .map_err(|error| format!("マイク一覧を取得できません: {error}"))?
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| description.name() == requested_device)
                })
                .ok_or_else(|| format!("指定したマイクが見つかりません: {requested_device}"))?
        };
        let device_name = device
            .description()
            .map(|description| description.name().to_owned())
            .unwrap_or_else(|_| "不明なマイク".into());
        let supported = device
            .default_input_config()
            .map_err(|error| format!("マイク形式を取得できません: {error}"))?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let source_sample_rate = config.sample_rate;
        let channels = config.channels as usize;
        let stream = match sample_format {
            SampleFormat::F32 => build_input_stream(
                &device,
                &config,
                channels,
                source_sample_rate,
                target_sample_rate,
                sender,
                |sample: f32| sample,
            ),
            SampleFormat::I16 => build_input_stream(
                &device,
                &config,
                channels,
                source_sample_rate,
                target_sample_rate,
                sender,
                |sample: i16| sample as f32 / i16::MAX as f32,
            ),
            SampleFormat::U16 => build_input_stream(
                &device,
                &config,
                channels,
                source_sample_rate,
                target_sample_rate,
                sender,
                |sample: u16| sample as f32 / u16::MAX as f32 * 2.0 - 1.0,
            ),
            format => Err(format!("未対応のマイクサンプル形式です: {format:?}")),
        }?;
        stream
            .play()
            .map_err(|error| format!("マイク入力を開始できません: {error}"))?;
        Ok(Self {
            stream,
            device_name,
            source_sample_rate,
        })
    }

    pub fn stop(self) -> Result<(), String> {
        self.stream
            .pause()
            .map_err(|error| format!("マイク入力を停止できません: {error}"))
    }
}

fn build_input_stream<T: cpal::SizedSample + Copy + Send + 'static>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    source_sample_rate: u32,
    target_sample_rate: u32,
    sender: Sender<MicrophoneEvent>,
    convert: impl Fn(T) -> f32 + Send + 'static,
) -> Result<Stream, String> {
    let mut phase = 0_u64;
    let error_sender = sender.clone();
    device
        .build_input_stream(
            *config,
            move |data: &[T], _| {
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|frame| {
                        frame.iter().map(|sample| convert(*sample)).sum::<f32>()
                            / frame.len() as f32
                    })
                    .collect();
                let mut resampled = Vec::new();
                for sample in mono {
                    phase += target_sample_rate as u64;
                    while phase >= source_sample_rate as u64 {
                        resampled.push(sample);
                        phase -= source_sample_rate as u64;
                    }
                }
                if !resampled.is_empty() {
                    let _ = sender.send(MicrophoneEvent::Samples(resampled));
                }
            },
            move |error| {
                let _ = error_sender.send(MicrophoneEvent::Error(error.to_string()));
            },
            None,
        )
        .map_err(|error| format!("マイク入力ストリームを作成できません: {error}"))
}

impl VoiceActivityDetector {
    pub fn new(settings: VadSettings) -> Self {
        Self {
            settings,
            utterance: Vec::new(),
            silent_samples: 0,
        }
    }

    pub fn push(&mut self, samples: &[f32]) -> Option<Vec<f32>> {
        let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>()
            / samples.len().max(1) as f32)
            .sqrt();
        let voiced = 20.0 * rms.max(1.0e-9).log10() >= self.settings.volume_gate_db;
        if voiced || !self.utterance.is_empty() {
            self.utterance.extend_from_slice(samples);
        }
        self.silent_samples = if voiced {
            0
        } else {
            self.silent_samples + samples.len()
        };
        let reached_silence = self.silent_samples as f32
            >= self.settings.end_silence_seconds * self.settings.sample_rate as f32;
        let reached_max = self.utterance.len() as f32
            >= self.settings.max_seconds * self.settings.sample_rate as f32;
        if self.utterance.is_empty() || (!reached_silence && !reached_max) {
            return None;
        }
        let voiced_len = self.utterance.len().saturating_sub(self.silent_samples);
        let accepted =
            voiced_len as f32 >= self.settings.min_seconds * self.settings.sample_rate as f32;
        self.silent_samples = 0;
        let completed = std::mem::take(&mut self.utterance);
        accepted.then_some(completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formant_tone(f1: f32, f2: f32) -> Vec<f32> {
        (0..512)
            .map(|index| {
                let time = index as f32 / 16_000.0;
                0.45 * (2.0 * PI * f1 * time).sin() + 0.45 * (2.0 * PI * f2 * time).sin()
            })
            .collect()
    }

    #[test]
    fn classifies_synthetic_japanese_vowels() {
        let settings = LipSyncSettings::default();
        let expected = [
            MouthShape::A,
            MouthShape::I,
            MouthShape::U,
            MouthShape::E,
            MouthShape::O,
        ];
        let mut analyzer = LipSyncAnalyzer::new(settings.clone()).unwrap();
        for (formant, shape) in settings.formants.into_iter().zip(expected) {
            assert_eq!(
                analyzer.classify(&formant_tone(formant.f1, formant.f2)),
                shape
            );
        }
        assert_eq!(analyzer.classify(&[0.0; 512]), MouthShape::Close);
    }

    #[test]
    fn emits_only_when_smoothed_shape_changes() {
        let settings = LipSyncSettings {
            smoothing_frames: 1,
            ..LipSyncSettings::default()
        };
        let mut analyzer = LipSyncAnalyzer::new(settings).unwrap();
        let tone = formant_tone(775.0, 1175.0);
        assert_eq!(analyzer.analyze(&tone), Some(MouthShape::A));
        assert_eq!(analyzer.analyze(&tone), None);
        assert_eq!(analyzer.analyze(&[0.0; 512]), Some(MouthShape::Close));
    }

    #[test]
    fn vad_emits_a_bounded_utterance_after_silence() {
        let settings = VadSettings {
            end_silence_seconds: 0.1,
            min_seconds: 0.1,
            max_seconds: 1.0,
            ..VadSettings::default()
        };
        let mut vad = VoiceActivityDetector::new(settings);
        assert!(vad.push(&vec![0.2; 1_600]).is_none());
        let utterance = vad.push(&vec![0.0; 1_600]).unwrap();
        assert_eq!(utterance.len(), 3_200);
    }
}
