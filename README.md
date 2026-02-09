# lgtv

A command-line tool and Rust library for controlling LG WebOS TVs over the local network.

Supports power management, volume control, app launching, input switching, media playback, notifications, and more — all via WebSocket.

## Installation

### Pre-built binaries

Download the latest release from the [GitHub Releases](https://github.com/balcsida/lgtv-rs/releases) page. Binaries are available for:

- Linux (x86_64, aarch64)
- macOS (x86_64, Apple Silicon)
- Windows (x86_64)

### Homebrew (macOS / Linux)

```sh
brew install balcsida/tap/lgtv
```

### From source

Requires [Rust](https://www.rust-lang.org/tools/install) 2021 edition or later.

```sh
cargo install --git https://github.com/balcsida/lgtv-rs
```

Or clone and build locally:

```sh
git clone https://github.com/balcsida/lgtv-rs.git
cd lgtv-rs
cargo build --release
# Binary is at target/release/lgtv
```

## Quick start

### 1. Discover TVs on your network

```sh
lgtv scan
```

### 2. Pair with a TV

```sh
lgtv auth 192.168.1.100 living-room
```

Accept the pairing prompt on your TV. The client key is saved to the config file for future use.

### 3. Set a default TV

```sh
lgtv set-default living-room
```

### 4. Control your TV

```sh
lgtv off
lgtv set-volume 25
lgtv start-app netflix
lgtv send-button up up right enter
```

Use `-n` to target a specific TV when you have multiple configured:

```sh
lgtv -n bedroom off
```

## Commands

### Configuration

| Command | Description |
|---|---|
| `scan` | Discover LG TVs on the network via SSDP |
| `auth <host> <name>` | Pair with a TV and store credentials |
| `set-default <name>` | Set the default TV |
| `serialise` | Display stored TV configuration |

### Power

| Command | Description |
|---|---|
| `on` | Power on via Wake-on-LAN |
| `off` | Power off |
| `screen-off` | Turn screen off (standby) |
| `screen-on` | Wake screen from standby |
| `get-power-state` | Get current power state |

### Audio

| Command | Description |
|---|---|
| `set-volume <level>` | Set volume (0-100) |
| `volume-up` / `volume-down` | Adjust volume |
| `mute <true\|false>` | Mute or unmute |
| `audio-status` | Get audio status |
| `audio-volume` | Get current volume |
| `get-sound-output` | Get current output device |
| `set-sound-output <device>` | Set output (`tv_speaker`, `external_arc`, `headphone`, etc.) |

### Channels

| Command | Description |
|---|---|
| `get-tv-channel` | Get current channel |
| `set-tv-channel <id>` | Switch to channel |
| `list-channels` | List available channels |
| `input-channel-up` / `input-channel-down` | Navigate channels |

### Apps

| Command | Description |
|---|---|
| `list-apps` | List installed apps |
| `list-launch-points` | List launch points |
| `start-app <id>` | Launch an app |
| `close-app <id>` | Close an app |
| `open-app-with-payload <id> <json>` | Launch app with custom payload |
| `get-foreground-app-info` | Get info about the current app |

### Media playback

| Command | Description |
|---|---|
| `input-media-play` | Play |
| `input-media-pause` | Pause |
| `input-media-stop` | Stop |
| `input-media-rewind` | Rewind |
| `input-media-fast-forward` | Fast forward |

### Browser and YouTube

| Command | Description |
|---|---|
| `open-browser-at <url>` | Open URL in TV browser |
| `open-youtube-id <id>` | Open YouTube video by ID |
| `open-youtube-url <url>` | Open YouTube URL |
| `open-youtube-legacy-id <id>` | Open video on legacy YouTube app |
| `open-youtube-legacy-url <url>` | Open URL on legacy YouTube app |

### Inputs

| Command | Description |
|---|---|
| `list-inputs` | List external inputs |
| `set-input <id>` | Switch input |
| `set-device-info <id> <name> <icon>` | Set input device name and icon |

### Notifications

| Command | Description |
|---|---|
| `notification <message>` | Show a toast notification |
| `notification-with-icon <message> <url>` | Show notification with an icon |
| `create-alert <title> <message> <button1> [buttons...]` | Show a dialog with buttons |
| `close-alert <id>` | Close a dialog by ID |

### Display

| Command | Description |
|---|---|
| `3d-on` / `3d-off` | Toggle 3D mode |
| `get-picture-settings` | Get picture settings |
| `set-picture-mode <mode>` | Set picture mode |

### Remote control

| Command | Description |
|---|---|
| `send-button <buttons...>` | Send button presses (e.g. `up`, `down`, `left`, `right`, `enter`, `back`, `home`, `exit`, `red`, `green`, `yellow`, `blue`) |
| `send-enter-key` | Send enter key |

### System

| Command | Description |
|---|---|
| `sw-info` | Get software version |
| `get-system-info` | Get system information |
| `list-services` | List available services |

## Global options

| Flag | Description |
|---|---|
| `-n, --name <name>` | Target a specific TV by name |
| `--ssl` | Use encrypted connection (port 3001) |
| `-d, --debug` | Enable debug logging |

## Configuration file

TV credentials and settings are stored in JSON at one of these locations (in order of preference):

- `$XDG_CONFIG_HOME/lgtv/config.json`
- `~/.config/lgtv/config.json`
- `~/.lgtv/config.json`
- `/etc/lgtv/config.json`

Example config:

```json
{
  "_default": "living-room",
  "living-room": {
    "key": "client-key-from-pairing",
    "mac": "AA:BB:CC:DD:EE:FF",
    "ip": "192.168.1.100",
    "hostname": "LGwebOSTV.local"
  }
}
```

## Library usage

The crate can also be used as a Rust library:

```rust
use lgtv::{LgtvRemote, LgtvAuth, scan::scan_for_tvs};

#[tokio::main]
async fn main() -> lgtv::Result<()> {
    // Discover TVs
    let tvs = scan_for_tvs().await?;

    // Connect and control
    let mut remote = LgtvRemote::connect("192.168.1.100", false).await?;
    remote.send_command("ssap://audio/setVolume", Some(serde_json::json!({"volume": 25}))).await?;

    Ok(())
}
```

## License

MIT
