use crate::error::{LgtvError, Result};
use crate::payload;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;
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
        let client_key = key.ok_or_else(|| LgtvError::AuthError("Client key is required".to_string()))?;
        
        let ip_addr = match ip {
            Some(ip) => ip.to_string(),
            None => match hostname {
                Some(host) => {
                    let socket_addr = (host, 0)
                        .to_socket_addrs()?
                        .next()
                        .ok_or_else(|| LgtvError::ConnectionError(format!("Could not resolve hostname: {}", host)))?;
                    socket_addr.ip().to_string()
                }
                None => return Err(LgtvError::ConnectionError("Either IP or hostname is required".to_string())),
            },
        };
        
        let mac_address = match mac {
            Some(mac) => Some(mac.to_string()),
            None => None,
        };
        
        Ok(Self {
            client_key: client_key.to_string(),
            mac_address,
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
        
        // Create channel for sending messages to WebSocket
        let (tx, mut rx) = mpsc::channel::<Message>(32);
        self.ws_tx = Some(tx);
        
        // Create channel for handling responses
        let (response_tx, mut response_rx) = mpsc::channel::<Value>(32);
        
        let handshake_done = self.handshake_done.clone();
        let response_channels = self.response_channels.clone();
        
        // Handle the WebSocket connection in a separate task
        let (mut ws_writer, mut ws_reader) = ws_stream.split();
        
        // Writer task
        let _writer_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if ws_writer.send(msg).await.is_err() {
                    break;
                }
            }
        });
        
        // Reader task
        let _reader_task = tokio::spawn(async move {
            while let Some(msg) = ws_reader.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            log::debug!("Received response: {}", json);
                            
                            // Handle response by ID
                            if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                                let mut channels = response_channels.lock().await;
                                if let Some(tx) = channels.get(id) {
                                    if tx.send(json.clone()).await.is_err() {
                                        channels.remove(id);
                                    }
                                } else {
                                    // Send to general response channel
                                    let _ = response_tx.send(json.clone()).await;
                                }
                            } else {
                                // Send to general response channel
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
            Err(LgtvError::ConnectionError("WebSocket not connected".to_string()))
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
            return Err(LgtvError::CommandError("Handshake not completed".to_string()));
        }
        
        // Create message ID
        let message_id = match prefix {
            Some(p) => format!("{}_{}",  p, self.command_count),
            None => self.command_count.to_string(),
        };
        self.command_count += 1;
        
        // Create command message
        let mut message_data = json!({
            "id": message_id,
            "type": msg_type,
            "uri": uri
        });
        
        if let Some(p) = payload {
            message_data["payload"] = p;
        }
        
        // Create channel for response
        let (tx, rx) = mpsc::channel::<Value>(1);
        self.response_channels.lock().await.insert(message_id.clone(), tx);
        
        // Send command
        self.send_message(message_data.to_string()).await?;
        
        Ok(rx)
    }
    
    pub async fn on(&self) -> Result<()> {
        if self.mac_address.is_none() {
            return Err(LgtvError::CommandError(
                "MAC address is required for power on".to_string()
            ));
        }
        
        let mac_str = self.mac_address.as_ref().unwrap();
        
        // Parse MAC address string into bytes
        let mac_bytes = Self::parse_mac_address(mac_str).map_err(|e| {
            LgtvError::CommandError(format!("Invalid MAC address format: {}", e))
        })?;
        
        // Create and send magic packet
        let magic_packet = MagicPacket::new(&mac_bytes);
        magic_packet.send().map_err(|e| {
            LgtvError::CommandError(format!("Failed to send Wake-on-LAN packet: {}", e))
        })?;
        
        Ok(())
    }
    
    // Helper function to parse MAC address string into [u8; 6]
    fn parse_mac_address(mac_str: &str) -> std::result::Result<[u8; 6], String> {
        let parts: Vec<&str> = mac_str.split(|c| c == ':' || c == '-').collect();
        
        if parts.len() != 6 {
            return Err(format!("MAC address should have 6 parts, found {}", parts.len()));
        }
        
        let mut mac_bytes = [0u8; 6];
        
        for (i, part) in parts.iter().enumerate() {
            mac_bytes[i] = u8::from_str_radix(part, 16)
                .map_err(|_| format!("Invalid hex value: {}", part))?;
        }
        
        Ok(mac_bytes)
    }
    
    pub async fn off(&mut self) -> Result<()> {
        let mut rx = self.send_command("request", "ssap://system/turnOff", None, None).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Power off response: {}", response);
        }
        
        Ok(())
    }
    
    pub async fn mute(&mut self, muted: bool) -> Result<()> {
        let payload = json!({"mute": muted});
        let mut rx = self.send_command("request", "ssap://audio/setMute", Some(payload), None).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Mute response: {}", response);
        }
        
        Ok(())
    }
    
    pub async fn set_volume(&mut self, level: u32) -> Result<()> {
        let payload = json!({"volume": level});
        let mut rx = self.send_command("request", "ssap://audio/setVolume", Some(payload), None).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Set volume response: {}", response);
        }
        
        Ok(())
    }
    
    pub async fn volume_up(&mut self) -> Result<()> {
        let mut rx = self.send_command("request", "ssap://audio/volumeUp", None, Some("volumeup")).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Volume up response: {}", response);
        }
        
        Ok(())
    }
    
    pub async fn volume_down(&mut self) -> Result<()> {
        let mut rx = self.send_command("request", "ssap://audio/volumeDown", None, Some("volumedown")).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Volume down response: {}", response);
        }
        
        Ok(())
    }
    
    // Many more command methods would be implemented here...
    // For brevity, I'm including only a subset of the commands
    
    pub async fn input_media_play(&mut self) -> Result<()> {
        let mut rx = self.send_command("request", "ssap://media.controls/play", None, None).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Media play response: {}", response);
        }
        
        Ok(())
    }
    
    pub async fn input_media_stop(&mut self) -> Result<()> {
        let mut rx = self.send_command("request", "ssap://media.controls/stop", None, None).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Media stop response: {}", response);
        }
        
        Ok(())
    }
    
    pub async fn input_media_pause(&mut self) -> Result<()> {
        let mut rx = self.send_command("request", "ssap://media.controls/pause", None, None).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Media pause response: {}", response);
        }
        
        Ok(())
    }
    
    pub async fn notification(&mut self, message: &str) -> Result<()> {
        let payload = json!({"message": message});
        let mut rx = self.send_command(
            "request", 
            "ssap://system.notifications/createToast", 
            Some(payload), 
            None
        ).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Notification response: {}", response);
        }
        
        Ok(())
    }
    
    pub async fn open_browser_at(&mut self, url: &str) -> Result<()> {
        let payload = json!({"target": url});
        let mut rx = self.send_command(
            "request", 
            "ssap://system.launcher/open", 
            Some(payload), 
            None
        ).await?;
        
        if let Some(response) = rx.recv().await {
            log::debug!("Open browser response: {}", response);
        }
        
        Ok(())
    }
    
    // Additional methods would follow the same pattern
}
