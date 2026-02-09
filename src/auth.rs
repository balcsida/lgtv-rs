use crate::error::{LgtvError, Result};
use crate::payload;
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::net::{IpAddr, ToSocketAddrs};
use std::str::FromStr;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};

pub struct LgtvAuth {
    client_key: Option<String>,
    mac_address: Option<String>,
    ip: String,
    hostname: Option<String>,
    ssl: bool,
    handshake_done: bool,
}

impl LgtvAuth {
    pub fn new(_name: &str, host: &str, ssl: bool) -> Result<Self> {
        let ip: String;
        let hostname: Option<String>;

        // Check if host is an IP address or hostname
        if let Ok(ip_addr) = IpAddr::from_str(host) {
            ip = ip_addr.to_string();

            // We can't easily get hostname from IP in standard library
            // Just keep it as None
            hostname = None;
        } else {
            hostname = Some(host.to_string());

            // Try to resolve hostname to IP
            let socket_addr = (host, 0).to_socket_addrs()?.next().ok_or_else(|| {
                LgtvError::ConnectionError(format!("Could not resolve hostname: {}", host))
            })?;
            ip = socket_addr.ip().to_string();
        }

        // MAC address retrieval is tricky in pure Rust
        // For now, just leave it as None, but in production code
        // you might want to use a platform-specific solution
        let mac_address = None;

        Ok(Self {
            client_key: None,
            mac_address,
            ip,
            hostname,
            ssl,
            handshake_done: false,
        })
    }

    pub async fn connect(&mut self) -> Result<()> {
        let ws_url = if self.ssl {
            format!("wss://{}:3001/", self.ip)
        } else {
            format!("ws://{}:3000/", self.ip)
        };

        let (ws_stream, _) = connect_async(ws_url).await?;

        let (tx, mut rx) = mpsc::channel::<Value>(32);

        // Handle the WebSocket connection in a separate task
        self.handle_connection(ws_stream, tx).await?;

        // Wait for pairing response
        println!("Please accept the pairing request on your LG TV");
        while let Some(response) = rx.recv().await {
            if let Some(payload) = response.get("payload") {
                if let Some(client_key) = payload.get("client-key") {
                    if let Some(key) = client_key.as_str() {
                        self.client_key = Some(key.to_string());
                        self.handshake_done = true;
                        break;
                    }
                }
            }
        }

        if self.client_key.is_none() {
            return Err(LgtvError::AuthError("Pairing failed".to_string()));
        }

        Ok(())
    }

    async fn handle_connection(
        &self,
        mut ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        tx: mpsc::Sender<Value>,
    ) -> Result<()> {
        // Send hello data
        let hello_data = payload::hello_data();
        ws_stream
            .send(Message::Text(hello_data.to_string()))
            .await?;

        // Process responses
        tokio::spawn(async move {
            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            if tx.send(json).await.is_err() {
                                break;
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

        Ok(())
    }

    pub fn serialise(&self) -> Value {
        json!({
            "key": self.client_key,
            "mac": self.mac_address,
            "ip": self.ip,
            "hostname": self.hostname
        })
    }
}
