use clap::{Parser, Subcommand};
use lgtv::{
    auth::LgtvAuth,
    config::{find_config, read_config, write_config},
    cursor::LgtvCursor,
    error::Result,
    remote::LgtvRemote,
    scan::scan_for_tvs,
};
use serde_json::{json, Value};
use std::process::exit;

#[derive(Parser)]
#[clap(
    name = "lgtv",
    about = "LG WebOS TV Controller",
    version = env!("CARGO_PKG_VERSION"),
    author = "Karl Lattimer <karl@qdh.org.uk> and Rust port contributors",
)]
struct Cli {
    /// TV Name to use from config
    #[clap(short, long)]
    name: Option<String>,

    /// Use SSL for connection
    #[clap(long)]
    ssl: bool,

    /// Enable debug output
    #[clap(short, long)]
    debug: bool,

    /// Command to execute
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan for LG TVs on the network
    Scan,

    /// Authenticate with a TV
    Auth {
        /// TV IP address or hostname
        host: String,
        /// Name to give to the TV
        name: String,
    },

    /// Set a TV as the default
    SetDefault {
        /// TV name
        name: String,
    },

    // ── Power ──────────────────────────────────

    /// Power on the TV (via Wake-on-LAN)
    On,

    /// Power off the TV
    Off,

    /// Turn the screen off (standby)
    ScreenOff,

    /// Turn the screen on
    ScreenOn,

    /// Get the current power state
    GetPowerState,

    // ── Audio ──────────────────────────────────

    /// Mute/unmute the TV
    Mute {
        /// Mute state (true/false)
        muted: bool,
    },

    /// Set volume level
    SetVolume {
        /// Volume level (0-100)
        level: u32,
    },

    /// Volume up
    VolumeUp,

    /// Volume down
    VolumeDown,

    /// Get audio status
    AudioStatus,

    /// Get current volume
    AudioVolume,

    /// Get current sound output device
    GetSoundOutput,

    /// Set sound output device (tv_speaker, external_arc, headphone, etc.)
    SetSoundOutput {
        /// Output device name
        output: String,
    },

    // ── TV Channels ───────────────────────────

    /// Get the current TV channel
    GetTvChannel,

    /// Set the TV channel
    SetTvChannel {
        /// Channel ID
        channel_id: String,
    },

    /// List available channels
    ListChannels,

    /// Channel up
    InputChannelUp,

    /// Channel down
    InputChannelDown,

    // ── Media Controls ────────────────────────

    /// Media play
    InputMediaPlay,

    /// Media pause
    InputMediaPause,

    /// Media stop
    InputMediaStop,

    /// Media rewind
    InputMediaRewind,

    /// Media fast forward
    InputMediaFastForward,

    // ── Input Switching ───────────────────────

    /// List external inputs (HDMI, etc.)
    ListInputs,

    /// Switch to an input
    SetInput {
        /// Input ID
        input_id: String,
    },

    /// Set device info for an input
    SetDeviceInfo {
        /// Device ID
        id: String,
        /// Icon name
        icon: String,
        /// Label
        label: String,
    },

    // ── Applications ──────────────────────────

    /// List installed apps
    ListApps,

    /// List launch points
    ListLaunchPoints,

    /// Launch an app
    StartApp {
        /// App ID
        app_id: String,
    },

    /// Close an app
    CloseApp {
        /// App ID
        app_id: String,
    },

    /// Launch an app with a custom JSON payload
    OpenAppWithPayload {
        /// JSON payload string
        payload: String,
    },

    /// Get info about the foreground app
    GetForegroundAppInfo,

    // ── Browser & YouTube ─────────────────────

    /// Open a URL in the browser
    OpenBrowserAt {
        /// URL to open
        url: String,
    },

    /// Open YouTube by video ID
    OpenYoutubeId {
        /// YouTube video ID
        video_id: String,
    },

    /// Open YouTube by URL
    OpenYoutubeUrl {
        /// YouTube URL
        url: String,
    },

    /// Open YouTube (legacy app) by video ID
    OpenYoutubeLegacyId {
        /// YouTube video ID
        video_id: String,
    },

    /// Open YouTube (legacy app) by URL
    OpenYoutubeLegacyUrl {
        /// YouTube URL
        url: String,
    },

    // ── Notifications ─────────────────────────

    /// Send a notification message
    Notification {
        /// Message to display
        message: String,
    },

    /// Send a notification with an icon
    NotificationWithIcon {
        /// Message to display
        message: String,
        /// URL of the icon image
        icon_url: String,
    },

    /// Create an alert dialog
    CreateAlert {
        /// Message to display
        message: String,
        /// Button label(s) as JSON array, e.g. '["OK","Cancel"]'
        buttons: String,
    },

    /// Close an alert dialog
    CloseAlert {
        /// Alert ID
        alert_id: String,
    },

    // ── 3D Display ────────────────────────────

    /// Enable 3D mode
    #[clap(name = "3d-on")]
    Input3dOn,

    /// Disable 3D mode
    #[clap(name = "3d-off")]
    Input3dOff,

    // ── Picture Settings ──────────────────────

    /// Get picture settings
    GetPictureSettings,

    /// Set picture mode
    SetPictureMode {
        /// Picture mode name
        mode: String,
    },

    // ── System Info ───────────────────────────

    /// Get software version info
    SwInfo,

    /// Get system info
    GetSystemInfo,

    /// List available services
    ListServices,

    // ── Misc ──────────────────────────────────

    /// Send the enter key
    SendEnterKey,

    /// Send button presses to the TV
    SendButton {
        /// Button names (e.g., up, down, left, right, etc.)
        #[clap(required = true)]
        buttons: Vec<String>,
    },

    /// Print stored config for the TV
    Serialise,
}

/// Print a JSON value as pretty-printed output.
fn print_response(value: &Value) {
    if let Ok(s) = serde_json::to_string_pretty(value) {
        println!("{}", s);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Configure logging
    if cli.debug {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    match &cli.command {
        Commands::Scan => {
            let results = scan_for_tvs().await?;

            if !results.is_empty() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "result": "ok",
                        "count": results.len(),
                        "list": results
                    }))?
                );
                exit(0);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "result": "failed",
                        "count": 0
                    }))?
                );
                exit(1);
            }
        }

        Commands::Auth { host, name } => {
            let config_path = find_config()?;
            let mut config = read_config(&config_path).unwrap_or_else(|_| json!({}));

            let mut auth = LgtvAuth::new(name, host, cli.ssl)?;
            auth.connect().await?;

            config[name] = auth.serialise();
            write_config(&config_path, &config)?;

            println!("Wrote config file: {}", config_path.display());
            exit(0);
        }

        Commands::SetDefault { name } => {
            let config_path = find_config()?;
            let mut config = match read_config(&config_path) {
                Ok(c) => c,
                Err(_) => {
                    println!("No config file found");
                    exit(1);
                }
            };

            if config.get(name).is_none() {
                println!("TV not found in config");
                exit(1);
            }

            config["_default"] = json!(name);
            write_config(&config_path, &config)?;

            println!("Wrote default to config file: {}", config_path.display());
            exit(0);
        }

        // Commands that require a TV configuration
        _ => {
            let tv_name = match &cli.name {
                Some(name) => name.clone(),
                None => {
                    let config_path = find_config()?;
                    let config = match read_config(&config_path) {
                        Ok(c) => c,
                        Err(_) => {
                            println!("No config file found");
                            exit(1);
                        }
                    };

                    match config.get("_default").and_then(|v| v.as_str()) {
                        Some(default_name) => default_name.to_string(),
                        None => {
                            println!("A TV name is required. Set one with -n/--name or the set-default command.");
                            exit(1);
                        }
                    }
                }
            };

            let config_path = find_config()?;
            let config = match read_config(&config_path) {
                Ok(c) => c,
                Err(_) => {
                    println!("No config file found");
                    exit(1);
                }
            };

            let tv_config = match config.get(&tv_name) {
                Some(c) => c,
                None => {
                    println!(
                        "No entry with the name '{}' was found in the configuration at {}.",
                        tv_name,
                        config_path.display()
                    );
                    exit(1);
                }
            };

            let ip = tv_config.get("ip").and_then(|v| v.as_str());
            let mac = tv_config.get("mac").and_then(|v| v.as_str());
            let key = tv_config.get("key").and_then(|v| v.as_str());
            let hostname = tv_config.get("hostname").and_then(|v| v.as_str());

            match &cli.command {
                Commands::SendButton { buttons } => {
                    let mut cursor =
                        LgtvCursor::new(&tv_name, ip, mac, key, hostname, cli.ssl).await?;
                    cursor
                        .execute(buttons.iter().map(|s| s.as_str()).collect())
                        .await?;
                }

                Commands::Serialise => {
                    print_response(tv_config);
                }

                // All commands that use the remote
                _ => {
                    let mut remote =
                        LgtvRemote::new(&tv_name, ip, mac, key, hostname, cli.ssl)?;

                    match &cli.command {
                        // ── Power ─────────────────────────────
                        Commands::On => match remote.on().await {
                            Ok(_) => println!("Power on command sent successfully"),
                            Err(e) => {
                                if e.to_string().contains("MAC address is required") {
                                    println!("Error: MAC address is required for power on. Please run 'lgtv scan' and then 'lgtv auth' to get the MAC address.");
                                } else {
                                    println!("Error: {}", e);
                                }
                                exit(1);
                            }
                        },
                        Commands::Off => {
                            remote.connect().await?;
                            remote.off().await?;
                        }
                        Commands::ScreenOff => {
                            remote.connect().await?;
                            remote.screen_off().await?;
                        }
                        Commands::ScreenOn => {
                            remote.connect().await?;
                            remote.screen_on().await?;
                        }
                        Commands::GetPowerState => {
                            remote.connect().await?;
                            let resp = remote.get_power_state().await?;
                            print_response(&resp);
                        }

                        // ── Audio ─────────────────────────────
                        Commands::Mute { muted } => {
                            remote.connect().await?;
                            remote.mute(*muted).await?;
                        }
                        Commands::SetVolume { level } => {
                            remote.connect().await?;
                            remote.set_volume(*level).await?;
                        }
                        Commands::VolumeUp => {
                            remote.connect().await?;
                            remote.volume_up().await?;
                        }
                        Commands::VolumeDown => {
                            remote.connect().await?;
                            remote.volume_down().await?;
                        }
                        Commands::AudioStatus => {
                            remote.connect().await?;
                            let resp = remote.audio_status().await?;
                            print_response(&resp);
                        }
                        Commands::AudioVolume => {
                            remote.connect().await?;
                            let resp = remote.audio_volume().await?;
                            print_response(&resp);
                        }
                        Commands::GetSoundOutput => {
                            remote.connect().await?;
                            let resp = remote.get_sound_output().await?;
                            print_response(&resp);
                        }
                        Commands::SetSoundOutput { output } => {
                            remote.connect().await?;
                            remote.set_sound_output(output).await?;
                        }

                        // ── TV Channels ───────────────────────
                        Commands::GetTvChannel => {
                            remote.connect().await?;
                            let resp = remote.get_tv_channel().await?;
                            print_response(&resp);
                        }
                        Commands::SetTvChannel { channel_id } => {
                            remote.connect().await?;
                            remote.set_tv_channel(channel_id).await?;
                        }
                        Commands::ListChannels => {
                            remote.connect().await?;
                            let resp = remote.list_channels().await?;
                            print_response(&resp);
                        }
                        Commands::InputChannelUp => {
                            remote.connect().await?;
                            remote.input_channel_up().await?;
                        }
                        Commands::InputChannelDown => {
                            remote.connect().await?;
                            remote.input_channel_down().await?;
                        }

                        // ── Media Controls ────────────────────
                        Commands::InputMediaPlay => {
                            remote.connect().await?;
                            remote.input_media_play().await?;
                        }
                        Commands::InputMediaPause => {
                            remote.connect().await?;
                            remote.input_media_pause().await?;
                        }
                        Commands::InputMediaStop => {
                            remote.connect().await?;
                            remote.input_media_stop().await?;
                        }
                        Commands::InputMediaRewind => {
                            remote.connect().await?;
                            remote.input_media_rewind().await?;
                        }
                        Commands::InputMediaFastForward => {
                            remote.connect().await?;
                            remote.input_media_fast_forward().await?;
                        }

                        // ── Input Switching ───────────────────
                        Commands::ListInputs => {
                            remote.connect().await?;
                            let resp = remote.list_inputs().await?;
                            print_response(&resp);
                        }
                        Commands::SetInput { input_id } => {
                            remote.connect().await?;
                            remote.set_input(input_id).await?;
                        }
                        Commands::SetDeviceInfo { id, icon, label } => {
                            remote.connect().await?;
                            remote.set_device_info(id, icon, label).await?;
                        }

                        // ── Applications ──────────────────────
                        Commands::ListApps => {
                            remote.connect().await?;
                            let resp = remote.list_apps().await?;
                            print_response(&resp);
                        }
                        Commands::ListLaunchPoints => {
                            remote.connect().await?;
                            let resp = remote.list_launch_points().await?;
                            print_response(&resp);
                        }
                        Commands::StartApp { app_id } => {
                            remote.connect().await?;
                            remote.start_app(app_id).await?;
                        }
                        Commands::CloseApp { app_id } => {
                            remote.connect().await?;
                            remote.close_app(app_id).await?;
                        }
                        Commands::OpenAppWithPayload { payload } => {
                            let parsed: Value = serde_json::from_str(payload).map_err(|e| {
                                lgtv::error::LgtvError::CommandError(format!(
                                    "Invalid JSON payload: {}",
                                    e
                                ))
                            })?;
                            remote.connect().await?;
                            remote.open_app_with_payload(parsed).await?;
                        }
                        Commands::GetForegroundAppInfo => {
                            remote.connect().await?;
                            let resp = remote.get_foreground_app_info().await?;
                            print_response(&resp);
                        }

                        // ── Browser & YouTube ─────────────────
                        Commands::OpenBrowserAt { url } => {
                            remote.connect().await?;
                            remote.open_browser_at(url).await?;
                        }
                        Commands::OpenYoutubeId { video_id } => {
                            remote.connect().await?;
                            remote.open_youtube_id(video_id).await?;
                        }
                        Commands::OpenYoutubeUrl { url } => {
                            remote.connect().await?;
                            remote.open_youtube_url(url).await?;
                        }
                        Commands::OpenYoutubeLegacyId { video_id } => {
                            remote.connect().await?;
                            remote.open_youtube_legacy_id(video_id).await?;
                        }
                        Commands::OpenYoutubeLegacyUrl { url } => {
                            remote.connect().await?;
                            remote.open_youtube_legacy_url(url).await?;
                        }

                        // ── Notifications ─────────────────────
                        Commands::Notification { message } => {
                            remote.connect().await?;
                            remote.notification(message).await?;
                        }
                        Commands::NotificationWithIcon { message, icon_url } => {
                            remote.connect().await?;
                            remote.notification_with_icon(message, icon_url).await?;
                        }
                        Commands::CreateAlert { message, buttons } => {
                            let btn_value: Value =
                                serde_json::from_str(buttons).map_err(|e| {
                                    lgtv::error::LgtvError::CommandError(format!(
                                        "Invalid JSON for buttons: {}",
                                        e
                                    ))
                                })?;
                            remote.connect().await?;
                            let resp = remote.create_alert(message, btn_value).await?;
                            print_response(&resp);
                        }
                        Commands::CloseAlert { alert_id } => {
                            remote.connect().await?;
                            remote.close_alert(alert_id).await?;
                        }

                        // ── 3D Display ────────────────────────
                        Commands::Input3dOn => {
                            remote.connect().await?;
                            remote.input_3d_on().await?;
                        }
                        Commands::Input3dOff => {
                            remote.connect().await?;
                            remote.input_3d_off().await?;
                        }

                        // ── Picture Settings ──────────────────
                        Commands::GetPictureSettings => {
                            remote.connect().await?;
                            let resp = remote.get_picture_settings().await?;
                            print_response(&resp);
                        }
                        Commands::SetPictureMode { mode } => {
                            remote.connect().await?;
                            remote.set_picture_mode(mode).await?;
                        }

                        // ── System Info ───────────────────────
                        Commands::SwInfo => {
                            remote.connect().await?;
                            let resp = remote.sw_info().await?;
                            print_response(&resp);
                        }
                        Commands::GetSystemInfo => {
                            remote.connect().await?;
                            let resp = remote.get_system_info().await?;
                            print_response(&resp);
                        }
                        Commands::ListServices => {
                            remote.connect().await?;
                            let resp = remote.list_services().await?;
                            print_response(&resp);
                        }

                        // ── Misc ──────────────────────────────
                        Commands::SendEnterKey => {
                            remote.connect().await?;
                            remote.send_enter_key().await?;
                        }

                        // Already handled above
                        _ => unreachable!(),
                    }
                }
            }
        }
    }

    Ok(())
}
