//! # vnc_server
//!
//! A pure Rust VNC server library for Windows.
//!
//! ## Quick Start
//!
//! ```no_run
//! use vncrs::{VncServer, VncServerConfig};
//! use vncrs::capture::scrap::ScrapCapture;
//! use vncrs::input::enigo_input::EnigoInput;
//!
//! let config = VncServerConfig::new()
//!     .port(5900)
//!     .password("secret")
//!     .name("My Desktop");
//!
//! let capture = ScrapCapture::new().unwrap();
//! let input = EnigoInput::new();
//!
//! let mut server = VncServer::new(capture, input, config);
//! server.listen().unwrap();
//! ```

pub mod capture;
pub mod config;
pub mod encoding;
pub mod error;
pub mod input;
pub mod protocol;
pub mod server;
pub mod stats;

pub use config::VncServerConfig;
pub use error::{Result, VncError};
pub use server::VncServer;
