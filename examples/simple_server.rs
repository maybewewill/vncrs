use std::sync::atomic::Ordering;
use vncrs::capture::scrap::ScrapCapture;
use vncrs::input::enigo_input::EnigoInput;
use vncrs::{VncServer, VncServerConfig};

fn main() -> vncrs::Result<()> {
    let config = VncServerConfig::new()
        .port(5900)
        .password("secret")
        .max_fps(30);

    let capture = ScrapCapture::new()?;
    let input = EnigoInput::new();
    let mut server = VncServer::new(capture, input, config);

    let running = server.running_flag();
    ctrlc::set_handler(move || {
        running.store(false, Ordering::Relaxed);
    })
    .ok();

    server.listen()
}
