# P2P Voice

A peer-to-peer voice communication application built with Rust, featuring real-time audio streaming, automatic NAT traversal, and a modern GUI.

## Features

- **Peer-to-Peer Architecture**: Direct UDP communication between clients with no central media server
- **Automatic NAT Traversal**: Built-in STUN and hole-punching for seamless connectivity
- **Real-Time Audio**: Low-latency voice chat using Opus codec at 48kHz
- **Adaptive Jitter Buffer**: Dynamic buffer adjustment for smooth audio playback
- **Voice Activity Detection**: Automatic transmission based on voice activity with hangover prevention
- **Modern GUI**: Clean interface built with egui/eframe
- **Cross-Platform**: Supports Windows, Linux, and macOS (Intel & Apple Silicon)
- **Auto-Update**: Built-in update checker and downloader
- **Persistent Settings**: Saves configuration across sessions
- **Mute/Deafen Controls**: Quick access to privacy controls
- **Ping Monitoring**: Real-time latency display for each peer

## Technology Stack

- **Language**: Rust 2024 Edition
- **GUI Framework**: eframe/egui with Glow renderer
- **Audio Backend**: cpal (Cross-Platform Audio Library)
- **Audio Codec**: Opus (optimized for VoIP)
- **Networking**: 
  - WebSocket for signaling and peer discovery
  - UDP for direct peer-to-peer audio streaming
- **TLS**: rustls for secure connections

## Installation

### From Releases

Download the latest release for your platform from the [Releases page](https://github.com/tallfly/p2p-voice/releases):

- **Windows**: `p2p-voice-windows.exe`
- **Linux**: `p2p-voice-linux`
- **macOS ARM64** (Apple Silicon): `p2p-voice-macos-arm64`
- **macOS x86_64** (Intel): `p2p-voice-macos-x86_64`

### Building from Source

#### Prerequisites

**All Platforms:**
- Rust (stable toolchain)
- CMake

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install libasound2-dev pkg-config cmake build-essential libssl-dev
```

**Windows:**
- Visual Studio Build Tools with C++ support

**macOS:**
- Xcode Command Line Tools

#### Build Instructions

```bash
# Clone the repository
git clone https://github.com/tallfly/p2p-voice.git
cd p2p-voice

# Build in release mode
cargo build --release
```

The binary will be located at:
- **Windows**: `target/release/p2p-voice.exe`
- **Linux/macOS**: `target/release/p2p-voice`

## Usage

1. **Launch the application**
2. **Login/Register**: Enter your username and password to authenticate with the server
3. **Join a Room**: Enter a room name to connect with others
4. **Audio Devices**: Select your preferred input (microphone) and output (speakers) devices
5. **Communicate**: Start talking! The app automatically detects voice activity and transmits audio

### Controls

- **Mute**: Disable microphone transmission
- **Deafen**: Disable audio output (can't hear others)
- **Volume Control**: Adjust individual peer volumes
- **Overlay Mode**: Optional overlay display

### Configuration

Settings are automatically saved and persisted:
- Server URL (default: `p2p.tallfly.me`)
- Username and authentication token
- Selected audio devices
- Overlay preferences

## Project Structure

```
p2p-voice/
├── src/
│   ├── main.rs      # Application entry point
│   ├── app.rs       # Main application state and UI logic
│   ├── audio.rs     # Audio device enumeration and management
│   ├── engine.rs    # Voice engine with Opus encoding/decoding
│   ├── models.rs    # Data structures and peer state
│   ├── network.rs   # Networking (STUN, WebSocket, UDP)
│   ├── ui.rs        # User interface rendering
│   └── updater.rs   # Auto-update functionality
├── Cargo.toml       # Rust dependencies and build configuration
└── LICENSE          # MIT License
```

## Architecture

### Voice Engine

The voice engine handles:
- **Audio Capture**: Captures microphone input at hardware sample rate
- **Resampling**: Converts to 48kHz for Opus encoding
- **Encoding**: Compresses audio using Opus codec (VoIP optimized)
- **Transmission**: Sends encoded packets via UDP to all peers
- **Reception**: Receives packets from peers with sequence numbers
- **Jitter Buffer**: Adaptive buffering to handle network variability
- **Decoding**: Decompresses Opus packets back to PCM audio
- **Mixing**: Combines audio from multiple peers
- **Playback**: Outputs mixed audio to speakers

### Networking

1. **Signaling**: WebSocket connection to server for peer discovery
2. **NAT Traversal**: 
   - STUN query to discover public IP
   - Hole-punching packets to establish direct connections
3. **Direct Communication**: UDP sockets for low-latency audio streaming
4. **Keep-alive**: Ping/Pong mechanism for latency monitoring and connection health

### Packet Format

- **Audio Packets**: `[seq_num(2 bytes)][opus_payload]`
- **Hole Punch**: `HOLE_PUNCH` (10 bytes)
- **Ping**: `PING[timestamp(8 bytes)]` (12 bytes)
- **Pong**: `PONG[timestamp(8 bytes)]` (12 bytes)

## Development

### Running in Development Mode

```bash
cargo run
```

### Release Build Optimization

The release profile is configured with maximum optimizations:
- LTO (Link Time Optimization) enabled
- Single codegen unit for better optimization
- Panic abort for smaller binaries

## CI/CD

GitHub Actions automatically builds releases for:
- Windows (x86_64)
- Linux (x86_64)
- macOS (ARM64 and x86_64)

Triggered by pushing version tags (e.g., `v0.4.0`).

## License

MIT License - See [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

## Acknowledgments

- [Opus](https://opus-codec.org/) - Audio codec
- [cpal](https://github.com/RustAudio/cpal) - Audio I/O
- [eframe/egui](https://github.com/emilk/egui) - GUI framework
- [tungstenite](https://github.com/snapview/tungstenite-rs) - WebSocket implementation
