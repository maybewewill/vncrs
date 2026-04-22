use std::sync::atomic::Ordering;
use vncrs::capture::windows::WindowsCapture;
use vncrs::input::NoopInput;
use vncrs::{VncServer, VncServerConfig};

fn main() -> vncrs::Result<()> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    let config = VncServerConfig::new()
        .port(5900)
        .password("viewonly")
        .max_fps(144);

    let capture = WindowsCapture::new()?;
    let mut server = VncServer::new(capture, NoopInput, config);

    let running = server.running_flag();
    ctrlc::set_handler(move || {
        running.store(false, Ordering::Relaxed);
    })
    .ok();

    server.listen()
}
