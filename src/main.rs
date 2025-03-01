use clap::{Parser, Subcommand};
use lgtv::{
    auth::LgtvAuth,
    config::{find_config, read_config, write_config},
    cursor::LgtvCursor,
    error::Result,
    remote::LgtvRemote,
    scan::scan_for_tvs,
};
use serde_json::json;
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
    
    /// Power on the TV
    On,
    
    /// Power off the TV
    Off,
    
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
    
    /// Send a notification message
    Notification {
        /// Message to display
        message: String,
    },
    
    /// Open a URL in the browser
    OpenBrowserAt {
        /// URL to open
        url: String,
    },
    
    /// Send button presses to the TV
    SendButton {
        /// Button names (e.g., up, down, left, right, etc.)
        #[clap(required = true)]
        buttons: Vec<String>,
    },
    
    // Additional commands would be added here...
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
            
            // Store TV configuration
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
            
            if !config.get(name).is_some() {
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
                            println!("A TV name is required. Set one with -n/--name or the setDefault command.");
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
                    println!("No entry with the name '{}' was found in the configuration at {}.", 
                      tv_name, config_path.display());
                    exit(1);
                }
            };
            
            let ip = tv_config.get("ip").and_then(|v| v.as_str());
            let mac = tv_config.get("mac").and_then(|v| v.as_str());
            let key = tv_config.get("key").and_then(|v| v.as_str());
            let hostname = tv_config.get("hostname").and_then(|v| v.as_str());
            
            match &cli.command {
                Commands::SendButton { buttons } => {
                    let mut cursor = LgtvCursor::new(
                        &tv_name, 
                        ip, 
                        mac, 
                        key, 
                        hostname, 
                        cli.ssl
                    ).await?;
                    
                    cursor.execute(buttons.iter().map(|s| s.as_str()).collect()).await?;
                }
                
                // Handle TV commands that use the remote
                _ => {
                    let mut remote = LgtvRemote::new(
                        &tv_name, 
                        ip, 
                        mac, 
                        key, 
                        hostname, 
                        cli.ssl
                    )?;
                    
                    match &cli.command {
                        Commands::On => {
                            match remote.on().await {
                                Ok(_) => println!("Power on command sent successfully"),
                                Err(e) => {
                                    if e.to_string().contains("MAC address is required") {
                                        println!("Error: MAC address is required for power on. Please run 'lgtv scan' and then 'lgtv auth' to get the MAC address.");
                                    } else {
                                        println!("Error: {}", e);
                                    }
                                    exit(1);
                                }
                            }
                        }
                        Commands::Off => {
                            remote.connect().await?;
                            remote.off().await?;
                        }
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
                        Commands::Notification { message } => {
                            remote.connect().await?;
                            remote.notification(message).await?;
                        }
                        Commands::OpenBrowserAt { url } => {
                            remote.connect().await?;
                            remote.open_browser_at(url).await?;
                        }
                        // Additional remote command handlers would go here
                        _ => {}
                    }
                }
            }
        }
    }
    
    Ok(())
}
