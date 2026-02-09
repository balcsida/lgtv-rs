use crate::error::{LgtvError, Result};
use crate::remote::LgtvRemote;
use futures_util::SinkExt;
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};

pub struct LgtvCursor {
    websocket: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl LgtvCursor {
    pub async fn new(
        name: &str,
        ip: Option<&str>,
        mac: Option<&str>,
        key: Option<&str>,
        hostname: Option<&str>,
        ssl: bool,
    ) -> Result<Self> {
        // Create a remote to get the cursor socket
        let mut remote = LgtvRemote::new(name, ip, mac, key, hostname, ssl)?;
        remote.connect().await?;

        // Get cursor socket
        let mut socket_path = None;
        let mut rx = remote
            .send_command(
                "request",
                "ssap://com.webos.service.networkinput/getPointerInputSocket",
                None,
                None,
            )
            .await?;

        if let Some(response) = rx.recv().await {
            if let Some(payload) = response.get("payload") {
                if let Some(path) = payload.get("socketPath").and_then(|v| v.as_str()) {
                    socket_path = Some(path.to_string());
                }
            }
        }

        let socket_path = socket_path.ok_or_else(|| {
            LgtvError::CommandError("Failed to get cursor socket path".to_string())
        })?;

        // Connect to cursor socket
        let (websocket, _) = connect_async(socket_path).await?;

        Ok(Self {
            websocket: Some(websocket),
        })
    }

    async fn send_button(&mut self, button_data: &str) -> Result<()> {
        if let Some(ws) = &mut self.websocket {
            ws.send(Message::Text(button_data.to_string())).await?;
            Ok(())
        } else {
            Err(LgtvError::ConnectionError(
                "WebSocket not connected".to_string(),
            ))
        }
    }

    pub async fn execute(&mut self, buttons: Vec<&str>) -> Result<()> {
        if buttons.is_empty() {
            let possible_buttons = self.list_possible_buttons();
            println!(
                "Add button presses to perform. Possible options: {}",
                possible_buttons.join(", ")
            );
            return Ok(());
        }

        for (i, button) in buttons.iter().enumerate() {
            match *button {
                "up" => self.up().await?,
                "down" => self.down().await?,
                "left" => self.left().await?,
                "right" => self.right().await?,
                "click" => self.click().await?,
                "back" => self.back().await?,
                "enter" => self.enter().await?,
                "home" => self.home().await?,
                "exit" => self.exit().await?,
                "red" => self.red().await?,
                "green" => self.green().await?,
                "yellow" => self.yellow().await?,
                "blue" => self.blue().await?,
                "channel_up" => self.channel_up().await?,
                "channel_down" => self.channel_down().await?,
                "volume_up" => self.volume_up().await?,
                "volume_down" => self.volume_down().await?,
                "play" => self.play().await?,
                "pause" => self.pause().await?,
                "stop" => self.stop().await?,
                "rewind" => self.rewind().await?,
                "fast_forward" => self.fast_forward().await?,
                "asterisk" => self.asterisk().await?,
                _ => {
                    println!("{} is not a possible button press, skipped", button);
                    continue;
                }
            }

            if i != 0 {
                sleep(Duration::from_millis(100)).await;
            }
        }

        Ok(())
    }

    fn list_possible_buttons(&self) -> Vec<String> {
        vec![
            "up".to_string(),
            "down".to_string(),
            "left".to_string(),
            "right".to_string(),
            "click".to_string(),
            "back".to_string(),
            "enter".to_string(),
            "home".to_string(),
            "exit".to_string(),
            "red".to_string(),
            "green".to_string(),
            "yellow".to_string(),
            "blue".to_string(),
            "channel_up".to_string(),
            "channel_down".to_string(),
            "volume_up".to_string(),
            "volume_down".to_string(),
            "play".to_string(),
            "pause".to_string(),
            "stop".to_string(),
            "rewind".to_string(),
            "fast_forward".to_string(),
            "asterisk".to_string(),
        ]
    }

    pub async fn up(&mut self) -> Result<()> {
        self.send_button("type:button\nname:UP\n\n").await
    }

    pub async fn down(&mut self) -> Result<()> {
        self.send_button("type:button\nname:DOWN\n\n").await
    }

    pub async fn left(&mut self) -> Result<()> {
        self.send_button("type:button\nname:LEFT\n\n").await
    }

    pub async fn right(&mut self) -> Result<()> {
        self.send_button("type:button\nname:RIGHT\n\n").await
    }

    pub async fn click(&mut self) -> Result<()> {
        self.send_button("type:click\n\n\n").await
    }

    pub async fn back(&mut self) -> Result<()> {
        self.send_button("type:button\nname:BACK\n\n").await
    }

    pub async fn enter(&mut self) -> Result<()> {
        self.send_button("type:button\nname:ENTER\n\n").await
    }

    pub async fn home(&mut self) -> Result<()> {
        self.send_button("type:button\nname:HOME\n\n").await
    }

    pub async fn exit(&mut self) -> Result<()> {
        self.send_button("type:button\nname:EXIT\n\n").await
    }

    pub async fn red(&mut self) -> Result<()> {
        self.send_button("type:button\nname:RED\n\n").await
    }

    pub async fn green(&mut self) -> Result<()> {
        self.send_button("type:button\nname:GREEN\n\n").await
    }

    pub async fn yellow(&mut self) -> Result<()> {
        self.send_button("type:button\nname:YELLOW\n\n").await
    }

    pub async fn blue(&mut self) -> Result<()> {
        self.send_button("type:button\nname:BLUE\n\n").await
    }

    pub async fn channel_up(&mut self) -> Result<()> {
        self.send_button("type:button\nname:CHANNELUP\n\n").await
    }

    pub async fn channel_down(&mut self) -> Result<()> {
        self.send_button("type:button\nname:CHANNELDOWN\n\n").await
    }

    pub async fn volume_up(&mut self) -> Result<()> {
        self.send_button("type:button\nname:VOLUMEUP\n\n").await
    }

    pub async fn volume_down(&mut self) -> Result<()> {
        self.send_button("type:button\nname:VOLUMEDOWN\n\n").await
    }

    pub async fn play(&mut self) -> Result<()> {
        self.send_button("type:button\nname:PLAY\n\n").await
    }

    pub async fn pause(&mut self) -> Result<()> {
        self.send_button("type:button\nname:PAUSE\n\n").await
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.send_button("type:button\nname:STOP\n\n").await
    }

    pub async fn rewind(&mut self) -> Result<()> {
        self.send_button("type:button\nname:REWIND\n\n").await
    }

    pub async fn fast_forward(&mut self) -> Result<()> {
        self.send_button("type:button\nname:FASTFORWARD\n\n").await
    }

    pub async fn asterisk(&mut self) -> Result<()> {
        self.send_button("type:button\nname:ASTERISK\n\n").await
    }
}
