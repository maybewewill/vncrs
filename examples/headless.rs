use std::sync::atomic::Ordering;
use vnc_server::capture::scrap::ScrapCapture;
use vnc_server::input::NoopInput;
use vnc_server::{VncServer, VncServerConfig};

fn main() -> vnc_server::Result<()> {
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
