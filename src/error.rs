use thiserror::Error;

#[derive(Error, Debug)]
pub enum VncError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Handshake failed: {0}")]
    Handshake(String),

    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(String),

    #[error("Screen capture error: {0}")]
    Capture(String),

    #[error("Encoding error: {0}")]
    Encoding(String),
}

pub type Result<T> = std::result::Result<T, VncError>;
