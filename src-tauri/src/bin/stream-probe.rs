use std::{env, time::Duration};

use local_vtuber_studio::stream::{AvatarState, start_obs_server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cycles = env::args()
        .nth(1)
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(15);
    let root = env::current_dir()?;
    let server = start_obs_server(
        root.join("ui-stream"),
        root.join("ui/shared"),
        root.clone(),
        58090,
        58099,
        state("a", "smile", "a"),
    )
    .await?;
    println!(
        "{}",
        serde_json::json!({"event": "obs_started", "url": format!("http://127.0.0.1:{}/", server.port())})
    );
    let sequence = [
        state("a", "smile", "a"),
        state("c", "sad", "o"),
        state("d", "blink", "a"),
    ];
    for avatar in sequence.into_iter().cycle().take(cycles) {
        server.publish(avatar);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    server.stop().await?;
    println!("{}", serde_json::json!({"event": "obs_stopped"}));
    Ok(())
}

fn state(character: &str, expression: &str, mouth: &str) -> AvatarState {
    let directory = match character {
        "a" => "projected-36",
        "c" => "projected-c-36",
        "d" => "projected-d-36",
        _ => unreachable!(),
    };
    let texture_root = format!("/assets/temp/t3/{directory}");
    AvatarState {
        expression_key: expression.into(),
        mouth_key: mouth.into(),
        model_url: "/assets/src-tauri/tests/fixtures/known-face.gltf".into(),
        texture_url: format!("{texture_root}/{expression}/{mouth}.png"),
        blink_texture_url: Some(format!("{texture_root}/blink/{mouth}.png")),
        crossfade_ms: 500,
        blink_min_ms: 800,
        blink_max_ms: 800,
        blink_duration_ms: 400,
        idle_sway_degrees: 2.0,
        ..AvatarState::default()
    }
}
