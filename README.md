# vncrs 🖥️

[![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![crates.io](https://img.shields.io/crates/v/vncrs.svg)](https://crates.io/crates/vncrs)

A pure [Rust](https://www.rust-lang.org/) VNC server library for Windows. Share your screen over the network to any standard VNC viewer — with zero fluff and minimal setup.

## 🌟 Features

* **RFB 3.8 Protocol:** Compatible with all major VNC clients — RealVNC, TigerVNC, UltraVNC, and more.
* **VNC Authentication:** Optional DES-based password protection.
* **Multiple Encodings:** Automatically negotiates the best encoding — Raw, Hextile, or Zlib.
* **Tile-Based Dirty Detection:** Only transmits changed screen regions to keep bandwidth low.
* **Full Input Injection:** Mouse movement, left/middle/right clicks, 4-directional scroll, and keyboard events.
* **View-Only Mode:** Serve your screen without allowing any remote input.
* **Configurable FPS Cap:** Tune frame rate anywhere from 1 to 120 FPS.
* **Graceful Shutdown:** Stop the server programmatically via an `Arc<AtomicBool>` flag.
* **Extensible Traits:** Plug in your own screen capture or input backend.
* **Per-Session Stats:** FPS and bandwidth reported after each client disconnects.

> **⚠️ Windows Only.** Screen capture and input injection rely on [`scrap`](https://crates.io/crates/scrap) and [`enigo`](https://crates.io/crates/enigo), which target Windows in this configuration.

## 🛠️ Prerequisites

To build and run this project from source, you will need Rust and Cargo installed on your system.

If you don't have Rust installed, get it via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## 🚀 Installation

### As a library dependency

```bash
cargo add vncrs
```

Or add it manually to your `Cargo.toml`:

```toml
[dependencies]
vncrs = "0.1.1"
```

### From source

Clone the repository and build with Cargo:

```bash
git clone https://github.com/yourusername/vncrs.git
cd vncrs
cargo build --release
```

## ⚡ Quick Start

```rust
use vncrs::{VncServer, VncServerConfig};
use vncrs::capture::scrap::ScrapCapture;
use vncrs::input::enigo_input::EnigoInput;

fn main() -> vncrs::Result<()> {
    let config = VncServerConfig::new()
        .port(5900)
        .password("secret")
        .name("My Desktop")
        .max_fps(30);

    let capture = ScrapCapture::new()?;
    let input = EnigoInput::new();

    let mut server = VncServer::new(capture, input, config);
    server.listen()
}
```

Then connect with any VNC viewer:

```bash
vncviewer 127.0.0.1:5900
```

## 🎮 Examples

Three ready-to-run examples are included in the `examples/` directory.

### simple_server

Minimal server on port 5900 with a password and 30 FPS cap.

```bash
cargo run --example simple_server
```

### headless

View-only server — the screen is shared but remote input is completely disabled.

```bash
cargo run --example headless
```

### full_server

A fully-featured CLI server with argument parsing.

```bash
cargo run --example full_server -- [OPTIONS]
```

| Flag | Default | Description |
|---|---|---|
| `-p, --port <PORT>` | `5900` | TCP port to listen on |
| `--password <PASS>` | — | VNC password (max 8 chars) |
| `-n, --name <NAME>` | `"Rust VNC"` | Desktop name shown to the client |
| `--fps <FPS>` | `60` | Max frame rate |
| `--view-only` | — | Disable remote input |
| `-v, --verbose` | — | Enable debug logging |

Example:

```bash
cargo run --example full_server -- --port 5901 --password hunter2 --fps 30
```

## ⚙️ Configuration

`VncServerConfig` uses a builder pattern — all fields have sensible defaults.

| Method | Default | Description |
|---|---|---|
| `.port(u16)` | `5900` | TCP port to listen on |
| `.password(&str)` | `None` | VNC password (truncated to 8 chars automatically) |
| `.name(&str)` | `"Rust VNC"` | Desktop name shown to the client |
| `.max_fps(u32)` | `60` | Frame rate cap (clamped to 1–120) |
| `.tile_size(usize)` | `64` | Dirty-check tile size in pixels (clamped to 16–256) |

```rust
let config = VncServerConfig::new()
    .port(5901)
    .password("pass1234")
    .name("Dev Machine")
    .max_fps(60)
    .tile_size(32);
```

## 🛑 Graceful Shutdown

The server exposes an `Arc<AtomicBool>` that you can flip from any thread — for example in a Ctrl-C handler:

```rust
use std::sync::atomic::Ordering;

let running = server.running_flag();

ctrlc::set_handler(move || {
    running.store(false, Ordering::Relaxed);
}).ok();

server.listen()?;
```

You can also call `server.stop()` directly if you hold a reference to the server.

## 🏗️ Architecture

```
vncrs
├── server.rs           VncServer — main accept / client loop
├── config.rs           VncServerConfig builder
├── error.rs            VncError + Result type alias
├── stats.rs            Per-session FPS / bandwidth counter
├── capture/
│   ├── mod.rs          ScreenCapture trait
│   └── scrap.rs        Windows implementation via `scrap`
├── input/
│   ├── mod.rs          InputHandler trait + NoopInput
│   ├── enigo_input.rs  Windows implementation via `enigo`
│   └── keysym.rs       X11 keysym → enigo key mapping
├── encoding/
│   ├── mod.rs          EncoderSet (negotiation + dispatch)
│   ├── raw.rs          Raw encoding
│   ├── hextile.rs      Hextile encoding
│   └── zlib.rs         Zlib encoding
└── protocol/
    ├── handshake.rs    RFB 3.8 handshake
    ├── auth.rs         VNC authentication (DES challenge-response)
    ├── messages.rs     Client message parsing
    └── pixel_format.rs PixelFormat negotiation
```

### Key Traits

Implement `ScreenCapture` to provide your own frame source:

```rust
pub trait ScreenCapture {
    fn width(&self) -> u16;
    fn height(&self) -> u16;
    fn stride(&self) -> usize;
    fn capture_frame(&mut self) -> Result<Option<Vec<u8>>>;
}
```

Implement `InputHandler` to handle remote input events:

```rust
pub trait InputHandler {
    fn move_mouse(&mut self, x: u16, y: u16);
    fn mouse_button(&mut self, button: u8, pressed: bool);
    fn scroll(&mut self, direction: ScrollDirection);
    fn key_event(&mut self, keysym: u32, down: bool);
}
```

Use the built-in `NoopInput` for a view-only server without writing a custom implementation.

## 📦 Dependencies

| Crate | Purpose |
|---|---|
| [`scrap`](https://crates.io/crates/scrap) | Screen capture |
| [`enigo`](https://crates.io/crates/enigo) | Mouse & keyboard injection |
| [`flate2`](https://crates.io/crates/flate2) | Zlib compression |
| [`des`](https://crates.io/crates/des) + [`cipher`](https://crates.io/crates/cipher) | DES for VNC challenge-response auth |
| [`rand`](https://crates.io/crates/rand) | Random challenge generation |
| [`thiserror`](https://crates.io/crates/thiserror) | Ergonomic error definitions |
| [`log`](https://crates.io/crates/log) + [`env_logger`](https://crates.io/crates/env_logger) | Structured logging |

## 🤝 Contributing

Contributions, issues, and feature requests are always welcome! Feel free to check the [issues page](https://github.com/yourusername/vncrs/issues) if you want to contribute.

1. Fork the project.
2. Create your feature branch (`git checkout -b feature/AmazingFeature`).
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`).
4. Push to the branch (`git push origin feature/AmazingFeature`).
5. Open a Pull Request.

## 📝 License

This project is licensed under the MIT License — see the `LICENSE` file for details.

---

*Developed with ❤️ in Rust*
