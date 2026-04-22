use std::sync::atomic::Ordering;
use vncrs::capture::windows::WindowsCapture;
use vncrs::capture::ScreenCapture;
use vncrs::input::enigo_input::EnigoInput;
use vncrs::input::InputHandler;
use vncrs::input::NoopInput;
use vncrs::{VncServer, VncServerConfig};

fn local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn main() -> vncrs::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut port: u16 = 5900;
    let mut password: Option<String> = None;
    let mut name = "Rust VNC".to_string();
    let mut fps: u32 = 60;
    let mut view_only = false;
    let mut verbose = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                i += 1;
                port = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(5900);
            }
            "--password" | "--pass" => {
                i += 1;
                password = args.get(i).map(|s| s[..s.len().min(8)].to_string());
            }
            "--name" | "-n" => {
                i += 1;
                name = args.get(i).cloned().unwrap_or(name);
            }
            "--fps" => {
                i += 1;
                fps = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(60);
            }
            "--view-only" => {
                view_only = true;
            }
            "--verbose" | "-v" => {
                verbose = true;
            }
            "--help" | "-h" => {
                println!("Usage: full_server [OPTIONS]");
                println!("  -p, --port <PORT>      Listen port [default: 5900]");
                println!("      --password <PASS>  VNC password (max 8 chars)");
                println!("  -n, --name <NAME>      Desktop name");
                println!("      --fps <FPS>        Max FPS [default: 60]");
                println!("      --view-only        Disable input");
                println!("  -v, --verbose          Enable logging");
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    if verbose {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let mut config = VncServerConfig::new().port(port).name(&name).max_fps(fps);

    if let Some(ref p) = password {
        config = config.password(p);
    }

    let capture = WindowsCapture::new()?;
    let w = capture.width();
    let h = capture.height();
    let ip = local_ip().unwrap_or_else(|| "localhost".into());

    println!();
    println!("  🦀 Rust VNC Server");
    println!("  ──────────────────────────────");
    println!("  Screen     {}x{}", w, h);
    println!("  Port       {}", port);
    println!(
        "  Auth       {}",
        if password.is_some() {
            "password"
        } else {
            "none"
        }
    );
    println!(
        "  Input      {}",
        if view_only { "view-only" } else { "full" }
    );
    println!("  Max FPS    {}", fps);
    println!("  ──────────────────────────────");
    println!("  Connect    vncviewer {}:{}", ip, port);
    println!();

    if view_only {
        start(capture, NoopInput, config)
    } else {
        start(capture, EnigoInput::new(), config)
    }
}

fn start<C: ScreenCapture, I: InputHandler>(
    capture: C,
    input: I,
    config: VncServerConfig,
) -> vncrs::Result<()> {
    let mut server = VncServer::new(capture, input, config);
    let running = server.running_flag();
    ctrlc::set_handler(move || {
        println!("\n  Shutting down...");
        running.store(false, Ordering::Relaxed);
    })
    .ok();
    server.listen()
}
