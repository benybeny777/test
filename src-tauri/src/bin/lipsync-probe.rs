use std::{
    collections::BTreeMap,
    sync::mpsc,
    time::{Duration, Instant},
};

use local_vtuber_studio::lipsync::{
    LipSyncAnalyzer, LipSyncSettings, MicrophoneCapture, MicrophoneEvent,
};

fn main() -> Result<(), String> {
    let settings = LipSyncSettings::default();
    let mut analyzer = LipSyncAnalyzer::new(settings.clone())?;
    let (sender, receiver) = mpsc::channel();
    let microphone = MicrophoneCapture::start("", settings.sample_rate, sender)?;
    println!(
        "{}",
        serde_json::json!({
            "event": "microphone_started",
            "device": microphone.device_name,
            "source_sample_rate": microphone.source_sample_rate,
            "analysis_sample_rate": settings.sample_rate,
        })
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut pending = Vec::new();
    let mut windows = 0_u32;
    let mut counts = BTreeMap::new();
    let mut errors = Vec::new();
    while Instant::now() < deadline {
        if let Ok(event) = receiver.recv_timeout(Duration::from_millis(100)) {
            match event {
                MicrophoneEvent::Samples(chunk) => {
                    pending.extend(chunk);
                    while pending.len() >= settings.window_samples {
                        let shape = analyzer.classify(&pending[..settings.window_samples]);
                        *counts.entry(format!("{shape:?}")).or_insert(0_u32) += 1;
                        pending.drain(..settings.window_samples);
                        windows += 1;
                    }
                }
                MicrophoneEvent::Error(error) => errors.push(error),
            }
        }
    }
    microphone.stop()?;
    println!(
        "{}",
        serde_json::json!({"event": "microphone_stopped", "windows": windows, "shapes": counts, "errors": errors})
    );
    if windows == 0 {
        return Err("マイクから解析可能なサンプルを取得できませんでした".into());
    }
    Ok(())
}
