use std::{collections::BTreeMap, io, path::PathBuf};

use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{oneshot, watch},
    task::JoinHandle,
};
use tower_http::services::ServeDir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarState {
    pub expression_key: String,
    pub mouth_key: String,
    pub model_url: String,
    pub texture_url: String,
    pub blink_texture_url: Option<String>,
    pub crossfade_ms: u32,
    pub blink_min_ms: u32,
    pub blink_max_ms: u32,
    pub blink_duration_ms: u32,
    pub idle_sway_degrees: f32,
    pub idle_sway_period_ms: u32,
    pub yaw: f32,
    pub pitch: f32,
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub arm_pose: BTreeMap<String, [f32; 3]>,
}

impl Default for AvatarState {
    fn default() -> Self {
        Self {
            expression_key: "smile".into(),
            mouth_key: "close".into(),
            model_url: "/assets/model.vrm".into(),
            texture_url: "/assets/expressions/smile/close.png".into(),
            blink_texture_url: Some("/assets/expressions/blink/close.png".into()),
            crossfade_ms: 160,
            blink_min_ms: 2_800,
            blink_max_ms: 6_500,
            blink_duration_ms: 140,
            idle_sway_degrees: 0.7,
            idle_sway_period_ms: 4_200,
            yaw: 0.0,
            pitch: 0.0,
            scale: 1.0,
            offset_x: 0.0,
            offset_y: 0.0,
            arm_pose: BTreeMap::new(),
        }
    }
}

impl AvatarState {
    pub fn apply_animation_config(&mut self, config: &crate::config::AvatarConfig) {
        self.crossfade_ms = config.crossfade_ms;
        self.blink_min_ms = config.blink_min_ms;
        self.blink_max_ms = config.blink_max_ms;
        self.blink_duration_ms = config.blink_duration_ms;
        self.idle_sway_degrees = config.idle_sway_degrees;
        self.idle_sway_period_ms = config.idle_sway_period_ms;
    }
}

#[derive(Clone)]
struct ServerState {
    avatar: watch::Sender<AvatarState>,
}

#[derive(Debug)]
pub struct ObsServer {
    port: u16,
    state: watch::Sender<AvatarState>,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<io::Result<()>>,
}

impl ObsServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn publish(&self, state: AvatarState) {
        self.state.send_replace(state);
    }

    pub async fn stop(mut self) -> io::Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.task.await.map_err(io::Error::other)?
    }
}

pub async fn start_obs_server(
    web_root: PathBuf,
    shared_root: PathBuf,
    assets_root: PathBuf,
    port_start: u16,
    port_end: u16,
    initial_state: AvatarState,
) -> io::Result<ObsServer> {
    if port_start > port_end {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "OBSポート範囲が逆です",
        ));
    }
    let mut selected = None;
    for port in port_start..=port_end {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                selected = Some((port, listener));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {}
            Err(error) => return Err(error),
        }
    }
    let (port, listener) = selected.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrInUse,
            format!("ポート {port_start}〜{port_end} が全て使用中のため開始できませんでした"),
        )
    })?;
    let (state, _) = watch::channel(initial_state);
    let router = Router::new()
        .route("/state", get(upgrade_state_socket))
        .nest_service("/shared", ServeDir::new(shared_root))
        .nest_service("/assets", ServeDir::new(assets_root))
        .fallback_service(ServeDir::new(web_root).append_index_html_on_directories(true))
        .with_state(ServerState {
            avatar: state.clone(),
        });
    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_receiver.await;
            })
            .await
    });
    Ok(ObsServer {
        port,
        state,
        shutdown: Some(shutdown_sender),
        task,
    })
}

async fn upgrade_state_socket(
    State(state): State<ServerState>,
    upgrade: WebSocketUpgrade,
) -> Response {
    upgrade.on_upgrade(move |socket| stream_state(socket, state.avatar.subscribe()))
}

async fn stream_state(mut socket: WebSocket, mut state: watch::Receiver<AvatarState>) {
    loop {
        let json = match serde_json::to_string(&*state.borrow_and_update()) {
            Ok(json) => json,
            Err(_) => return,
        };
        if socket.send(Message::Text(json.into())).await.is_err() {
            return;
        }
        if state.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn serves_transparent_page_and_stops_cleanly() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("index.html"), "<body>stream</body>").unwrap();
        let assets = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let server = start_obs_server(
            root.path().to_owned(),
            shared.path().to_owned(),
            assets.path().to_owned(),
            58980,
            58989,
            AvatarState::default(),
        )
        .await
        .unwrap();
        let mut client = tokio::net::TcpStream::connect(("127.0.0.1", server.port()))
            .await
            .unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("<body>stream</body>"));
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn reports_when_the_entire_port_range_is_occupied() {
        let occupied = TcpListener::bind(("127.0.0.1", 58990)).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let assets = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let error = start_obs_server(
            root.path().to_owned(),
            shared.path().to_owned(),
            assets.path().to_owned(),
            58990,
            58990,
            AvatarState::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
        drop(occupied);
    }

    #[test]
    fn applies_animation_settings_to_stream_state() {
        let mut state = AvatarState::default();
        let config = crate::config::AvatarConfig {
            crossfade_ms: 250,
            idle_sway_degrees: 1.2,
            ..crate::config::AvatarConfig::default()
        };
        state.apply_animation_config(&config);
        assert_eq!(state.crossfade_ms, 250);
        assert_eq!(state.idle_sway_degrees, 1.2);
    }
}
