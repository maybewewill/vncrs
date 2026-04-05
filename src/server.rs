use crate::capture::ScreenCapture;
use crate::config::VncServerConfig;
use crate::encoding::EncoderSet;
use crate::error::Result;
use crate::input::{InputHandler, ScrollDirection};
use crate::protocol::{perform_handshake, ClientMessage, PixelFormat};
use crate::stats::FpsCounter;
use log::{info, warn};
use std::io::{BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

fn scale_frame(
    src: &[u8],
    src_stride: usize,
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
) -> (Vec<u8>, usize) {
    let dst_stride = dst_w * 4;
    let mut dst = vec![0u8; dst_stride * dst_h];
    for dy in 0..dst_h {
        let sy = (dy * src_h / dst_h).min(src_h.saturating_sub(1));
        for dx in 0..dst_w {
            let sx = (dx * src_w / dst_w).min(src_w.saturating_sub(1));
            let src_off = sy * src_stride + sx * 4;
            let dst_off = dy * dst_stride + dx * 4;
            if src_off + 4 <= src.len() {
                dst[dst_off..dst_off + 4].copy_from_slice(&src[src_off..src_off + 4]);
            }
        }
    }
    (dst, dst_stride)
}

enum ReaderEvent {
    Message(ClientMessage),
    Disconnected,
}

struct MouseState {
    buttons: u8,
}

impl MouseState {
    fn new() -> Self {
        Self { buttons: 0 }
    }

    fn scroll_events(&self, new_buttons: u8) -> Vec<ScrollDirection> {
        let mut scrolls = Vec::new();
        if new_buttons & 8 != 0 && self.buttons & 8 == 0 {
            scrolls.push(ScrollDirection::Up);
        }
        if new_buttons & 16 != 0 && self.buttons & 16 == 0 {
            scrolls.push(ScrollDirection::Down);
        }
        if new_buttons & 32 != 0 && self.buttons & 32 == 0 {
            scrolls.push(ScrollDirection::Left);
        }
        if new_buttons & 64 != 0 && self.buttons & 64 == 0 {
            scrolls.push(ScrollDirection::Right);
        }
        scrolls
    }

    fn button_changes(&mut self, new_buttons: u8) -> Vec<(u8, bool)> {
        let mut changes = Vec::new();
        let diff = self.buttons ^ new_buttons;
        if diff & 1 != 0 {
            changes.push((1, new_buttons & 1 != 0));
        }
        if diff & 2 != 0 {
            changes.push((2, new_buttons & 2 != 0));
        }
        if diff & 4 != 0 {
            changes.push((3, new_buttons & 4 != 0));
        }
        self.buttons = new_buttons;
        changes
    }
}

struct ClientFramebuffer {
    pixels: Vec<u8>,
    needs_full_update: bool,
}

impl ClientFramebuffer {
    fn new() -> Self {
        Self {
            pixels: Vec::new(),
            needs_full_update: true,
        }
    }

    fn apply_dirty_regions(&mut self, frame: &[u8], stride: usize, dirty: &[(u16, u16, u16, u16)]) {
        if self.pixels.len() != frame.len() {
            self.pixels = frame.to_vec();
            return;
        }
        for &(x, y, w, h) in dirty {
            for row in y as usize..(y + h) as usize {
                let start = row * stride + x as usize * 4;
                let end = start + w as usize * 4;
                if end <= frame.len() && end <= self.pixels.len() {
                    self.pixels[start..end].copy_from_slice(&frame[start..end]);
                }
            }
        }
    }

    fn find_dirty(
        &self,
        curr: &[u8],
        stride: usize,
        width: u16,
        height: u16,
        tile_size: usize,
    ) -> Vec<(u16, u16, u16, u16)> {
        if self.needs_full_update || self.pixels.is_empty() {
            return vec![(0, 0, width, height)];
        }

        let mut dirty = Vec::new();
        for ty in (0..height as usize).step_by(tile_size) {
            for tx in (0..width as usize).step_by(tile_size) {
                let tw = tile_size.min(width as usize - tx);
                let th = tile_size.min(height as usize - ty);

                let mut changed = false;
                for y in ty..ty + th {
                    let s = y * stride + tx * 4;
                    let e = s + tw * 4;
                    if e <= self.pixels.len() && e <= curr.len() && self.pixels[s..e] != curr[s..e]
                    {
                        changed = true;
                        break;
                    }
                }
                if changed {
                    dirty.push((tx as u16, ty as u16, tw as u16, th as u16));
                }
            }
        }
        dirty
    }
}

pub struct VncServer<C: ScreenCapture, I: InputHandler> {
    capture: C,
    encoder: EncoderSet,
    input: I,
    config: VncServerConfig,
    stats: FpsCounter,
    running: Arc<AtomicBool>,
}

impl<C: ScreenCapture, I: InputHandler> VncServer<C, I> {
    pub fn new(capture: C, input: I, config: VncServerConfig) -> Self {
        Self {
            capture,
            encoder: EncoderSet::new(),
            input,
            config,
            stats: FpsCounter::new(),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn running_flag(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    pub fn listen(&mut self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.config.port);
        let listener = TcpListener::bind(&addr)?;
        listener.set_nonblocking(true)?;

        if self.config.password.is_some() {
            println!("🔐 Password authentication enabled");
        } else {
            println!("⚠️  No password set");
        }
        println!("🚀 VNC server listening on {}", addr);
        println!();

        while self.running.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, addr)) => {
                    println!("✅ Client connected: {}", addr);

                    if let Err(e) = self.handle_connection(stream) {
                        println!("❌ Error: {}", e);
                    }

                    println!("👋 Client disconnected");
                    println!("📊 Session stats:");
                    self.stats.print_stats();
                    println!("\n⏳ Waiting for next client...\n");
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    warn!("Accept error: {}", e);
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }

        println!("🛑 Server shut down");
        Ok(())
    }

    fn handle_connection(&mut self, stream: TcpStream) -> Result<()> {
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;

        let reader_stream = stream.try_clone()?;
        let writer_stream = stream;

        let mut reader = BufReader::new(reader_stream.try_clone()?);
        let mut writer = BufWriter::with_capacity(1024 * 1024, writer_stream);

        let pf = PixelFormat::bgra32();

        let eff_w = self.config.width.unwrap_or(self.capture.width());
        let eff_h = self.config.height.unwrap_or(self.capture.height());

        perform_handshake(
            &mut reader,
            &mut writer,
            eff_w,
            eff_h,
            &self.config.name,
            &pf,
            self.config.password.as_deref(),
        )?;

        self.stats = FpsCounter::new();
        self.encoder = EncoderSet::new();

        reader_stream.set_read_timeout(None)?;

        self.client_loop(reader_stream, &mut writer, eff_w, eff_h)
    }

    fn client_loop<W: Write>(
        &mut self,
        reader_stream: TcpStream,
        writer: &mut W,
        eff_w: u16,
        eff_h: u16,
    ) -> Result<()> {
        let (tx, rx) = mpsc::channel::<ReaderEvent>();
        let running = self.running.clone();

        thread::spawn(move || {
            let mut reader = BufReader::new(reader_stream);
            loop {
                match ClientMessage::read_from(&mut reader) {
                    Ok(msg) => {
                        if tx.send(ReaderEvent::Message(msg)).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(ReaderEvent::Disconnected);
                        break;
                    }
                }
            }
        });

        let frame_interval = Duration::from_millis(self.config.frame_interval_ms());
        let tile_size = self.config.tile_size;
        let mut mouse = MouseState::new();
        let mut update_requested = false;
        let mut swap_rb = false;
        let mut client_fb = ClientFramebuffer::new();

        while running.load(Ordering::Relaxed) {
            let loop_start = Instant::now();

            loop {
                match rx.try_recv() {
                    Ok(ReaderEvent::Message(msg)) => match msg {
                        ClientMessage::SetPixelFormat { format } => {
                            let client_pf = PixelFormat::from_bytes(&format);
                            swap_rb = client_pf.needs_bgr_swap();
                            println!(
                                "🎨 Pixel format: r={} b={} (swap={})",
                                client_pf.red_shift, client_pf.blue_shift, swap_rb
                            );
                        }
                        ClientMessage::SetEncodings { encodings } => {
                            self.encoder.negotiate(&encodings);
                            info!("Encodings: {:?}", encodings);
                        }
                        ClientMessage::FramebufferUpdateRequest { incremental, .. } => {
                            update_requested = true;
                            if !incremental {
                                client_fb.needs_full_update = true;
                            }
                        }
                        ClientMessage::PointerEvent { buttons, x, y } => {
                            let native_x =
                                (x as u32 * self.capture.width() as u32 / eff_w as u32) as u16;
                            let native_y =
                                (y as u32 * self.capture.height() as u32 / eff_h as u32) as u16;
                            self.input.move_mouse(native_x, native_y);

                            for dir in mouse.scroll_events(buttons) {
                                self.input.scroll(dir);
                            }

                            for (btn, pressed) in mouse.button_changes(buttons) {
                                self.input.mouse_button(btn, pressed);
                            }
                        }
                        ClientMessage::KeyEvent { down, key } => {
                            self.input.key_event(key, down);
                        }
                        ClientMessage::ClientCutText { .. } => {}
                    },
                    Ok(ReaderEvent::Disconnected) => return Ok(()),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                }
            }

            if !update_requested {
                thread::sleep(Duration::from_millis(1));
                continue;
            }

            let raw_frame = match self.capture.capture_frame() {
                Ok(Some(f)) => f,
                Ok(None) => {
                    if client_fb.needs_full_update {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    if writer.write_all(&[0, 0, 0, 0]).is_err() {
                        return Ok(());
                    }
                    let _ = writer.flush();
                    update_requested = false;
                    self.stats.frame(4, 0);
                    let elapsed = loop_start.elapsed();
                    if elapsed < frame_interval {
                        thread::sleep(frame_interval - elapsed);
                    }
                    continue;
                }
                Err(e) => {
                    warn!("Capture error: {}", e);
                    thread::sleep(Duration::from_millis(50));
                    continue;
                }
            };

            let src_w = self.capture.width() as usize;
            let src_h = self.capture.height() as usize;
            let src_stride = self.capture.stride();

            let (frame, stride) = if eff_w as usize != src_w || eff_h as usize != src_h {
                scale_frame(
                    &raw_frame,
                    src_stride,
                    src_w,
                    src_h,
                    eff_w as usize,
                    eff_h as usize,
                )
            } else {
                (raw_frame, src_stride)
            };

            let w = eff_w;
            let h = eff_h;

            let dirty = client_fb.find_dirty(&frame, stride, w, h, tile_size);
            let dirty_count = dirty.len();
            let mut bytes_sent: usize = 4;

            if dirty.is_empty() {
                if writer.write_all(&[0, 0, 0, 0]).is_err() {
                    return Ok(());
                }
            } else {
                let mut header = [0u8; 4];
                header[2..4].copy_from_slice(&(dirty.len() as u16).to_be_bytes());
                if writer.write_all(&header).is_err() {
                    return Ok(());
                }

                for &(rx, ry, rw, rh) in &dirty {
                    let mut rect_header = [0u8; 12];
                    rect_header[0..2].copy_from_slice(&rx.to_be_bytes());
                    rect_header[2..4].copy_from_slice(&ry.to_be_bytes());
                    rect_header[4..6].copy_from_slice(&rw.to_be_bytes());
                    rect_header[6..8].copy_from_slice(&rh.to_be_bytes());
                    rect_header[8..12].copy_from_slice(&self.encoder.encoding_id().to_be_bytes());

                    if writer.write_all(&rect_header).is_err() {
                        return Ok(());
                    }

                    let data = self
                        .encoder
                        .encode_rect(&frame, stride, rx, ry, rw, rh, swap_rb)?;
                    bytes_sent += 12 + data.len();

                    if writer.write_all(&data).is_err() {
                        return Ok(());
                    }
                }

                if writer.flush().is_err() {
                    return Ok(());
                }

                client_fb.apply_dirty_regions(&frame, stride, &dirty);
                client_fb.needs_full_update = false;
            }

            if dirty.is_empty() {
                if writer.flush().is_err() {
                    return Ok(());
                }
            }

            update_requested = false;
            self.stats.frame(bytes_sent, dirty_count);

            let elapsed = loop_start.elapsed();
            if elapsed < frame_interval {
                thread::sleep(frame_interval - elapsed);
            }
        }

        Ok(())
    }
}
