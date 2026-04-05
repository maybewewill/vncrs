#[derive(Debug, Clone)]
pub struct VncServerConfig {
    pub port: u16,
    pub password: Option<String>,
    pub name: String,
    pub max_fps: u32,
    pub tile_size: usize,
}

impl VncServerConfig {
    pub fn new() -> Self {
        Self {
            port: 5900,
            password: None,
            name: "Rust VNC".to_string(),
            max_fps: 60,
            tile_size: 64,
        }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn password(mut self, password: &str) -> Self {
        self.password = Some(password[..password.len().min(8)].to_string());
        self
    }

    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn max_fps(mut self, fps: u32) -> Self {
        self.max_fps = fps.max(1).min(120);
        self
    }

    pub fn tile_size(mut self, size: usize) -> Self {
        self.tile_size = size.max(16).min(256);
        self
    }

    pub fn frame_interval_ms(&self) -> u64 {
        1000 / self.max_fps as u64
    }
}

impl Default for VncServerConfig {
    fn default() -> Self {
        Self::new()
    }
}
