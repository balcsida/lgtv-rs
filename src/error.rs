use std::net::AddrParseError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LgtvError {
    #[error("WebSocket error: {0}")]
    WebSocketError(Box<tokio_tungstenite::tungstenite::Error>),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Address parse error: {0}")]
    AddrParseError(#[from] AddrParseError),

    #[error("MAC address error: {0}")]
    MacAddressError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("No TV found with name: {0}")]
    TvNotFound(String),

    #[error("Command error: {0}")]
    CommandError(String),
}

impl From<tokio_tungstenite::tungstenite::Error> for LgtvError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        LgtvError::WebSocketError(Box::new(err))
    }
}

pub type Result<T> = std::result::Result<T, LgtvError>;
