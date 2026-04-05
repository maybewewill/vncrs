use std::sync::atomic::Ordering;
use vncrs::capture::scrap::ScrapCapture;
use vncrs::input::NoopInput;
use vncrs::{VncServer, VncServerConfig};

fn main() -> vncrs::Result<()> {
    let config = VncServerConfig::new()
        .port(5900)
        .password("viewonly")
        .max_fps(30);

    let capture = ScrapCapture::new()?;
    let mut server = VncServer::new(capture, NoopInput, config);

    let running = server.running_flag();
    ctrlc::set_handler(move || {
        running.store(false, Ordering::Relaxed);
    })
    .ok();

    server.listen()
}
