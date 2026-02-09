use crate::error::{LgtvError, Result};
use crate::payload;
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use wake_on_lan::MagicPacket;

pub struct LgtvRemote {
    client_key: String,
    mac_address: Option<String>,
    ip: String,
    hostname: Option<String>,
    name: String,
    command_count: u32,
    ssl: bool,
    handshake_done: Arc<Mutex<bool>>,
    response_channels: Arc<Mutex<HashMap<String, mpsc::Sender<Value>>>>,
    ws_tx: Option<mpsc::Sender<Message>>,
}

impl LgtvRemote {
    pub fn new(
        name: &str,
        ip: Option<&str>,
        mac: Option<&str>,
        key: Option<&str>,
        hostname: Option<&str>,
        ssl: bool,
    ) -> Result<Self> {
        let client_key =
            key.ok_or_else(|| LgtvError::AuthError("Client key is required".to_string()))?;

        let ip_addr = match ip {
            Some(ip) => ip.to_string(),
            None => match hostname {
                Some(host) => {
                    let socket_addr = (host, 0).to_socket_addrs()?.next().ok_or_else(|| {
                        LgtvError::ConnectionError(format!("Could not resolve hostname: {}", host))
                    })?;
                    socket_addr.ip().to_string()
                }
                None => {
                    return Err(LgtvError::ConnectionError(
                        "Either IP or hostname is required".to_string(),
                    ))
                }
            },
        };

        Ok(Self {
            client_key: client_key.to_string(),
            mac_address: mac.map(|m| m.to_string()),
            ip: ip_addr,
            hostname: hostname.map(|h| h.to_string()),
            name: name.to_string(),
            command_count: 0,
            ssl,
            handshake_done: Arc::new(Mutex::new(false)),
            response_channels: Arc::new(Mutex::new(HashMap::new())),
            ws_tx: None,
        })
    }

    pub async fn connect(&mut self) -> Result<()> {
        let ws_url = if self.ssl {
            format!("wss://{}:3001/", self.ip)
        } else {
            format!("ws://{}:3000/", self.ip)
        };

        let (ws_stream, _) = connect_async(ws_url).await?;

        let (tx, mut rx) = mpsc::channel::<Message>(32);
        self.ws_tx = Some(tx);

        let (response_tx, mut response_rx) = mpsc::channel::<Value>(32);

        let handshake_done = self.handshake_done.clone();
        let response_channels = self.response_channels.clone();

        let (mut ws_writer, mut ws_reader) = ws_stream.split();

        // Writer task
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if ws_writer.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // Reader task
        tokio::spawn(async move {
            while let Some(msg) = ws_reader.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            log::debug!("Received response: {}", json);

                            if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                                let mut channels = response_channels.lock().await;
                                if let Some(tx) = channels.get(id) {
                                    if tx.send(json.clone()).await.is_err() {
                                        channels.remove(id);
                                    }
                                } else {
                                    let _ = response_tx.send(json.clone()).await;
                                }
                            } else {
                                let _ = response_tx.send(json.clone()).await;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(e) => {
                        log::error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
        });

        // Send hello data for handshake
        let mut hello_data = payload::hello_data();
        hello_data["payload"]["client-key"] = json!(self.client_key);
        self.send_message(hello_data.to_string()).await?;

        // Wait for handshake response
        while let Some(response) = response_rx.recv().await {
            if let Some(payload) = response.get("payload") {
                if payload.get("client-key").is_some() {
                    log::debug!("Handshake complete");
                    let mut handshake = handshake_done.lock().await;
                    *handshake = true;
                    break;
                }
            }
        }

        Ok(())
    }

    async fn send_message(&self, message: String) -> Result<()> {
        if let Some(tx) = &self.ws_tx {
            tx.send(Message::Text(message)).await.map_err(|e| {
                LgtvError::ConnectionError(format!("Failed to send message: {}", e))
            })?;
            Ok(())
        } else {
            Err(LgtvError::ConnectionError(
                "WebSocket not connected".to_string(),
            ))
        }
    }

    pub async fn send_command(
        &mut self,
        msg_type: &str,
        uri: &str,
        payload: Option<Value>,
        prefix: Option<&str>,
    ) -> Result<mpsc::Receiver<Value>> {
        let handshake_done = *self.handshake_done.lock().await;
        if !handshake_done {
            return Err(LgtvError::CommandError(
                "Handshake not completed".to_string(),
            ));
        }

        let message_id = match prefix {
            Some(p) => format!("{}_{}", p, self.command_count),
            None => self.command_count.to_string(),
        };
        self.command_count += 1;

        let mut message_data = json!({
            "id": message_id,
            "type": msg_type,
            "uri": uri
        });

        if let Some(p) = payload {
            message_data["payload"] = p;
        }

        let (tx, rx) = mpsc::channel::<Value>(1);
        self.response_channels
            .lock()
            .await
            .insert(message_id.clone(), tx);

        self.send_message(message_data.to_string()).await?;

        Ok(rx)
    }

    /// Send a request and wait for the response payload.
    async fn send_request(
        &mut self,
        uri: &str,
        payload: Option<Value>,
        prefix: Option<&str>,
    ) -> Result<Value> {
        let mut rx = self.send_command("request", uri, payload, prefix).await?;
        match rx.recv().await {
            Some(response) => {
                log::debug!("Response: {}", response);
                Ok(response.get("payload").cloned().unwrap_or(json!({})))
            }
            None => Err(LgtvError::CommandError("No response received".to_string())),
        }
    }

    // ──────────────────────────────────────────────
    // Power
    // ──────────────────────────────────────────────

    pub async fn on(&self) -> Result<()> {
        let mac_str = self.mac_address.as_deref().ok_or_else(|| {
            LgtvError::CommandError("MAC address is required for power on".to_string())
        })?;

        let mac_bytes = Self::parse_mac_address(mac_str)
            .map_err(|e| LgtvError::CommandError(format!("Invalid MAC address format: {}", e)))?;

        let magic_packet = MagicPacket::new(&mac_bytes);
        magic_packet.send().map_err(|e| {
            LgtvError::CommandError(format!("Failed to send Wake-on-LAN packet: {}", e))
        })?;

        Ok(())
    }

    fn parse_mac_address(mac_str: &str) -> std::result::Result<[u8; 6], String> {
        let parts: Vec<&str> = mac_str.split([':', '-']).collect();
        if parts.len() != 6 {
            return Err(format!(
                "MAC address should have 6 parts, found {}",
                parts.len()
            ));
        }
        let mut mac_bytes = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            mac_bytes[i] =
                u8::from_str_radix(part, 16).map_err(|_| format!("Invalid hex value: {}", part))?;
        }
        Ok(mac_bytes)
    }

    pub async fn off(&mut self) -> Result<Value> {
        self.send_request("ssap://system/turnOff", None, None).await
    }

    pub async fn screen_off(&mut self) -> Result<Value> {
        self.send_request(
            "ssap://com.webos.service.tvpower/power/turnOffScreen",
            None,
            None,
        )
        .await
    }

    pub async fn screen_on(&mut self) -> Result<Value> {
        self.send_request(
            "ssap://com.webos.service.tvpower/power/turnOnScreen",
            None,
            None,
        )
        .await
    }

    pub async fn get_power_state(&mut self) -> Result<Value> {
        self.send_request(
            "ssap://com.webos.service.tvpower/power/getPowerState",
            None,
            Some("power"),
        )
        .await
    }

    // ──────────────────────────────────────────────
    // Audio
    // ──────────────────────────────────────────────

    pub async fn mute(&mut self, muted: bool) -> Result<Value> {
        self.send_request("ssap://audio/setMute", Some(json!({"mute": muted})), None)
            .await
    }

    pub async fn set_volume(&mut self, level: u32) -> Result<Value> {
        self.send_request(
            "ssap://audio/setVolume",
            Some(json!({"volume": level})),
            None,
        )
        .await
    }

    pub async fn volume_up(&mut self) -> Result<Value> {
        self.send_request("ssap://audio/volumeUp", None, Some("volumeup"))
            .await
    }

    pub async fn volume_down(&mut self) -> Result<Value> {
        self.send_request("ssap://audio/volumeDown", None, Some("volumedown"))
            .await
    }

    pub async fn audio_status(&mut self) -> Result<Value> {
        self.send_request("ssap://audio/getStatus", None, Some("status"))
            .await
    }

    pub async fn audio_volume(&mut self) -> Result<Value> {
        self.send_request("ssap://audio/getVolume", None, Some("volume"))
            .await
    }

    pub async fn get_sound_output(&mut self) -> Result<Value> {
        self.send_request(
            "ssap://com.webos.service.apiadapter/audio/getSoundOutput",
            None,
            None,
        )
        .await
    }

    pub async fn set_sound_output(&mut self, output: &str) -> Result<Value> {
        self.send_request(
            "ssap://audio/changeSoundOutput",
            Some(json!({"output": output})),
            None,
        )
        .await
    }

    // ──────────────────────────────────────────────
    // TV Channels
    // ──────────────────────────────────────────────

    pub async fn get_tv_channel(&mut self) -> Result<Value> {
        self.send_request("ssap://tv/getCurrentChannel", None, None)
            .await
    }

    pub async fn set_tv_channel(&mut self, channel_id: &str) -> Result<Value> {
        self.send_request(
            "ssap://tv/openChannel",
            Some(json!({"channelId": channel_id})),
            None,
        )
        .await
    }

    pub async fn list_channels(&mut self) -> Result<Value> {
        self.send_request("ssap://tv/getChannelList", None, Some("channels"))
            .await
    }

    pub async fn input_channel_up(&mut self) -> Result<Value> {
        self.send_request("ssap://tv/channelUp", None, None).await
    }

    pub async fn input_channel_down(&mut self) -> Result<Value> {
        self.send_request("ssap://tv/channelDown", None, None).await
    }

    // ──────────────────────────────────────────────
    // Media Controls
    // ──────────────────────────────────────────────

    pub async fn input_media_play(&mut self) -> Result<Value> {
        self.send_request("ssap://media.controls/play", None, None)
            .await
    }

    pub async fn input_media_pause(&mut self) -> Result<Value> {
        self.send_request("ssap://media.controls/pause", None, None)
            .await
    }

    pub async fn input_media_stop(&mut self) -> Result<Value> {
        self.send_request("ssap://media.controls/stop", None, None)
            .await
    }

    pub async fn input_media_rewind(&mut self) -> Result<Value> {
        self.send_request("ssap://media.controls/rewind", None, None)
            .await
    }

    pub async fn input_media_fast_forward(&mut self) -> Result<Value> {
        self.send_request("ssap://media.controls/fastForward", None, None)
            .await
    }

    // ──────────────────────────────────────────────
    // Input switching
    // ──────────────────────────────────────────────

    pub async fn list_inputs(&mut self) -> Result<Value> {
        self.send_request("ssap://tv/getExternalInputList", None, None)
            .await
    }

    pub async fn set_input(&mut self, input_id: &str) -> Result<Value> {
        self.send_request(
            "ssap://tv/switchInput",
            Some(json!({"inputId": input_id})),
            None,
        )
        .await
    }

    pub async fn set_device_info(&mut self, id: &str, icon: &str, label: &str) -> Result<Value> {
        self.send_request(
            "luna://com.webos.service.eim/setDeviceInfo",
            Some(json!({"id": id, "icon": icon, "label": label})),
            None,
        )
        .await
    }

    // ──────────────────────────────────────────────
    // Applications
    // ──────────────────────────────────────────────

    pub async fn list_apps(&mut self) -> Result<Value> {
        self.send_request("ssap://com.webos.applicationManager/listApps", None, None)
            .await
    }

    pub async fn list_launch_points(&mut self) -> Result<Value> {
        self.send_request(
            "ssap://com.webos.applicationManager/listLaunchPoints",
            None,
            None,
        )
        .await
    }

    pub async fn start_app(&mut self, app_id: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.launcher/launch",
            Some(json!({"id": app_id})),
            None,
        )
        .await
    }

    pub async fn close_app(&mut self, app_id: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.launcher/close",
            Some(json!({"id": app_id})),
            None,
        )
        .await
    }

    pub async fn open_app_with_payload(&mut self, payload: Value) -> Result<Value> {
        self.send_request(
            "ssap://com.webos.applicationManager/launch",
            Some(payload),
            None,
        )
        .await
    }

    pub async fn get_foreground_app_info(&mut self) -> Result<Value> {
        self.send_request(
            "ssap://com.webos.applicationManager/getForegroundAppInfo",
            None,
            None,
        )
        .await
    }

    // ──────────────────────────────────────────────
    // Browser & YouTube
    // ──────────────────────────────────────────────

    pub async fn open_browser_at(&mut self, url: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.launcher/open",
            Some(json!({"target": url})),
            None,
        )
        .await
    }

    pub async fn open_youtube_id(&mut self, video_id: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.launcher/launch",
            Some(json!({"id": "youtube.leanback.v4", "contentId": video_id})),
            None,
        )
        .await
    }

    pub async fn open_youtube_url(&mut self, url: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.launcher/launch",
            Some(json!({
                "id": "youtube.leanback.v4",
                "params": {"contentTarget": url}
            })),
            None,
        )
        .await
    }

    pub async fn open_youtube_legacy_id(&mut self, video_id: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.launcher/launch",
            Some(json!({"id": "com.webos.app.youtube", "contentId": video_id})),
            None,
        )
        .await
    }

    pub async fn open_youtube_legacy_url(&mut self, url: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.launcher/launch",
            Some(json!({
                "id": "com.webos.app.youtube",
                "params": {"contentTarget": url}
            })),
            None,
        )
        .await
    }

    // ──────────────────────────────────────────────
    // Notifications
    // ──────────────────────────────────────────────

    pub async fn notification(&mut self, message: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.notifications/createToast",
            Some(json!({"message": message})),
            None,
        )
        .await
    }

    pub async fn notification_with_icon(&mut self, message: &str, icon_url: &str) -> Result<Value> {
        let icon_data = Self::http_get_bytes(icon_url).await?;

        let icon_b64 = base64::engine::general_purpose::STANDARD.encode(&icon_data);
        let extension = icon_url.rsplit('.').next().unwrap_or("png");

        self.send_request(
            "ssap://system.notifications/createToast",
            Some(json!({
                "message": message,
                "iconData": icon_b64,
                "iconExtension": extension
            })),
            None,
        )
        .await
    }

    async fn http_get_bytes(url: &str) -> Result<Vec<u8>> {
        let url_body = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))
            .ok_or_else(|| LgtvError::CommandError("Invalid URL scheme".to_string()))?;

        let (host_port, path) = match url_body.find('/') {
            Some(i) => (&url_body[..i], &url_body[i..]),
            None => (url_body, "/"),
        };

        let host = host_port.split(':').next().unwrap_or(host_port);
        let port: u16 = if url.starts_with("https://") {
            443
        } else {
            host_port
                .split(':')
                .nth(1)
                .and_then(|p| p.parse().ok())
                .unwrap_or(80)
        };

        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, host
        );

        if url.starts_with("https://") {
            let tcp_stream = tokio::net::TcpStream::connect((host, port))
                .await
                .map_err(|e| LgtvError::CommandError(format!("Failed to connect: {}", e)))?;
            let connector = native_tls::TlsConnector::new()
                .map_err(|e| LgtvError::CommandError(format!("TLS error: {}", e)))?;
            let connector = tokio_native_tls::TlsConnector::from(connector);
            let mut stream = connector
                .connect(host, tcp_stream)
                .await
                .map_err(|e| LgtvError::CommandError(format!("TLS connect error: {}", e)))?;
            stream
                .write_all(request.as_bytes())
                .await
                .map_err(|e| LgtvError::CommandError(format!("Failed to send request: {}", e)))?;
            let mut response = Vec::new();
            stream
                .read_to_end(&mut response)
                .await
                .map_err(|e| LgtvError::CommandError(format!("Failed to read response: {}", e)))?;
            Self::extract_http_body(response)
        } else {
            let mut stream = tokio::net::TcpStream::connect((host, port))
                .await
                .map_err(|e| LgtvError::CommandError(format!("Failed to connect: {}", e)))?;
            stream
                .write_all(request.as_bytes())
                .await
                .map_err(|e| LgtvError::CommandError(format!("Failed to send request: {}", e)))?;
            let mut response = Vec::new();
            stream
                .read_to_end(&mut response)
                .await
                .map_err(|e| LgtvError::CommandError(format!("Failed to read response: {}", e)))?;
            Self::extract_http_body(response)
        }
    }

    fn extract_http_body(response: Vec<u8>) -> Result<Vec<u8>> {
        let header_end = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| LgtvError::CommandError("Invalid HTTP response".to_string()))?;
        Ok(response[header_end + 4..].to_vec())
    }

    pub async fn create_alert(&mut self, message: &str, buttons: Value) -> Result<Value> {
        self.send_request(
            "ssap://system.notifications/createAlert",
            Some(json!({
                "message": message,
                "buttons": buttons
            })),
            None,
        )
        .await
    }

    pub async fn close_alert(&mut self, alert_id: &str) -> Result<Value> {
        self.send_request(
            "ssap://system.notifications/closeAlert",
            Some(json!({"alertId": alert_id})),
            None,
        )
        .await
    }

    // ──────────────────────────────────────────────
    // 3D Display
    // ──────────────────────────────────────────────

    pub async fn input_3d_on(&mut self) -> Result<Value> {
        self.send_request("ssap://com.webos.service.tv.display/set3DOn", None, None)
            .await
    }

    pub async fn input_3d_off(&mut self) -> Result<Value> {
        self.send_request("ssap://com.webos.service.tv.display/set3DOff", None, None)
            .await
    }

    // ──────────────────────────────────────────────
    // Picture Settings
    // ──────────────────────────────────────────────

    pub async fn get_picture_settings(&mut self) -> Result<Value> {
        self.send_request(
            "ssap://settings/getSystemSettings",
            Some(json!({
                "category": "picture",
                "keys": ["contrast", "backlight", "brightness", "color", "pictureMode"]
            })),
            None,
        )
        .await
    }

    pub async fn set_picture_mode(&mut self, mode: &str) -> Result<Value> {
        self.send_request(
            "ssap://settings/setSystemSettings",
            Some(json!({
                "category": "picture",
                "settings": {"pictureMode": mode}
            })),
            None,
        )
        .await
    }

    // ──────────────────────────────────────────────
    // System Info
    // ──────────────────────────────────────────────

    pub async fn sw_info(&mut self) -> Result<Value> {
        self.send_request(
            "ssap://com.webos.service.update/getCurrentSWInformation",
            None,
            None,
        )
        .await
    }

    pub async fn get_system_info(&mut self) -> Result<Value> {
        self.send_request("ssap://system/getSystemInfo", None, None)
            .await
    }

    pub async fn list_services(&mut self) -> Result<Value> {
        self.send_request("ssap://api/getServiceList", None, None)
            .await
    }

    // ──────────────────────────────────────────────
    // IME
    // ──────────────────────────────────────────────

    pub async fn send_enter_key(&mut self) -> Result<Value> {
        self.send_request("ssap://com.webos.service.ime/sendEnterKey", None, None)
            .await
    }

    // ──────────────────────────────────────────────
    // Config serialization
    // ──────────────────────────────────────────────

    pub fn serialise(&self) -> Value {
        json!({
            "name": self.name,
            "ip": self.ip,
            "mac": self.mac_address,
            "key": self.client_key,
            "hostname": self.hostname
        })
    }
}
