use crate::capture::{CaptureRect, ScreenCapture};
use crate::config::VncServerConfig;
use crate::encoding::EncoderSet;
use crate::error::Result;
use crate::input::{InputHandler, ScrollDirection};
use crate::protocol::{perform_handshake, ClientMessage, PixelFormat};
use crate::stats::FpsCounter;
use log::{info, warn};
use socket2::SockRef;
use std::io::{BufReader, BufWriter, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

const WRITE_BUF_SIZE: usize = 2 * 1024 * 1024;
const READ_TIMEOUT_SECS: u64 = 30;
const WRITE_TIMEOUT_SECS: u64 = 10;
const ACCEPT_SLEEP_MS: u64 = 100;
const NO_FRAME_SLEEP_US: u64 = 500;
const IDLE_SLEEP_US: u64 = 1_000;
const CAPTURE_ERROR_SLEEP_MS: u64 = 50;
const EMPTY_UPDATE: [u8; 4] = [0, 0, 0, 0];

const BTN_LEFT: u8 = 1;
const BTN_MIDDLE: u8 = 2;
const BTN_RIGHT: u8 = 4;
const BTN_SCROLL_UP: u8 = 8;
const BTN_SCROLL_DOWN: u8 = 16;
const BTN_SCROLL_LEFT: u8 = 32;
const BTN_SCROLL_RIGHT: u8 = 64;

#[derive(Copy, Clone, Debug)]
struct DirtyRect {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

impl DirtyRect {
    #[inline]
    fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }
}

#[cfg(target_os = "windows")]
mod timer_res {
    extern "system" {
        fn timeBeginPeriod(uPeriod: u32) -> u32;
        fn timeEndPeriod(uPeriod: u32) -> u32;
    }

    pub struct HighResTimer;

    impl HighResTimer {
        pub fn acquire() -> Self {
            unsafe {
                timeBeginPeriod(1);
            }
            Self
        }
    }

    impl Drop for HighResTimer {
        fn drop(&mut self) {
            unsafe {
                timeEndPeriod(1);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod timer_res {
    pub struct HighResTimer;
    impl HighResTimer {
        pub fn acquire() -> Self {
            Self
        }
    }
}

struct ScaleContext {
    x_byte_offsets: Vec<usize>,
    y_row_offsets: Vec<usize>,
    dst_stride: usize,
    dst_w: usize,
    dst_h: usize,
}

impl ScaleContext {
    fn new(src_w: usize, src_h: usize, src_stride: usize, dst_w: usize, dst_h: usize) -> Self {
        let x_byte_offsets: Vec<usize> = (0..dst_w)
            .map(|dx| (dx * src_w / dst_w).min(src_w.saturating_sub(1)) * 4)
            .collect();
        let y_row_offsets: Vec<usize> = (0..dst_h)
            .map(|dy| (dy * src_h / dst_h).min(src_h.saturating_sub(1)) * src_stride)
            .collect();
        Self {
            x_byte_offsets,
            y_row_offsets,
            dst_stride: dst_w * 4,
            dst_w,
            dst_h,
        }
    }

    fn scale_into(&self, src: &[u8], dst: &mut Vec<u8>) {
        let total = self.dst_stride * self.dst_h;
        if dst.len() != total {
            dst.resize(total, 0);
        }

        let src_u32 = unsafe {
            std::slice::from_raw_parts(src.as_ptr() as *const u32, src.len() / 4)
        };
        let dst_u32 = unsafe {
            std::slice::from_raw_parts_mut(dst.as_mut_ptr() as *mut u32, dst.len() / 4)
        };

        let dst_w = self.dst_w;
        for (dy, &src_row_bytes) in self.y_row_offsets.iter().enumerate() {
            let src_row_px = src_row_bytes / 4;
            let dst_row_px = dy * dst_w;
            for dx in 0..dst_w {
                unsafe {
                    let sx = *self.x_byte_offsets.get_unchecked(dx) / 4;
                    *dst_u32.get_unchecked_mut(dst_row_px + dx) =
                        *src_u32.get_unchecked(src_row_px + sx);
                }
            }
        }
    }
}

#[inline]
fn row_differs(a: &[u8], b: &[u8]) -> bool {
    debug_assert_eq!(a.len(), b.len());
    let len = a.len();
    let chunks = len / 16;
    let tail = len % 16;
    for i in 0..chunks {
        let off = i * 16;
        unsafe {
            let av = std::ptr::read_unaligned(a.as_ptr().add(off) as *const u128);
            let bv = std::ptr::read_unaligned(b.as_ptr().add(off) as *const u128);
            if av != bv {
                return true;
            }
        }
    }
    if tail > 0 {
        let off = chunks * 16;
        if a[off..] != b[off..] {
            return true;
        }
    }
    false
}

#[inline]
fn tile_changed(
    curr: &[u8],
    prev: &[u8],
    stride: usize,
    tx: usize,
    ty: usize,
    tw: usize,
    th: usize,
) -> bool {
    let byte_w = tw * 4;
    for y in ty..ty + th {
        let s = y * stride + tx * 4;
        let e = s + byte_w;
        if e > curr.len() || e > prev.len() {
            return true;
        }
        if row_differs(&curr[s..e], &prev[s..e]) {
            return true;
        }
    }
    false
}

fn find_dirty_regions(
    curr: &[u8],
    prev: &[u8],
    stride: usize,
    width: u16,
    height: u16,
    tile_size: usize,
    out: &mut Vec<DirtyRect>,
) {
    out.clear();

    if prev.len() != curr.len() || prev.is_empty() {
        out.push(DirtyRect::new(0, 0, width, height));
        return;
    }

    let w = width as usize;
    let h = height as usize;

    for ty in (0..h).step_by(tile_size) {
        let th = tile_size.min(h - ty);
        let mut run_start: Option<usize> = None;
        let mut run_end: usize = 0;

        for tx in (0..w).step_by(tile_size) {
            let tw = tile_size.min(w - tx);
            if tile_changed(curr, prev, stride, tx, ty, tw, th) {
                match run_start {
                    Some(_) => run_end = tx + tw,
                    None => {
                        run_start = Some(tx);
                        run_end = tx + tw;
                    }
                }
            } else if let Some(start) = run_start.take() {
                out.push(DirtyRect::new(
                    start as u16,
                    ty as u16,
                    (run_end - start) as u16,
                    th as u16,
                ));
            }
        }
        if let Some(start) = run_start {
            out.push(DirtyRect::new(
                start as u16,
                ty as u16,
                (run_end - start) as u16,
                th as u16,
            ));
        }
    }
}

enum ReaderEvent {
    Message(ClientMessage),
    Disconnected,
}

fn spawn_reader_thread(stream: TcpStream) -> mpsc::Receiver<ReaderEvent> {
    let (tx, rx) = mpsc::channel::<ReaderEvent>();
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
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
    rx
}

struct SessionState {
    mouse_buttons: u8,
    update_requested: bool,
    needs_full_update: bool,
    swap_rb: bool,
}

impl SessionState {
    fn new() -> Self {
        Self {
            mouse_buttons: 0,
            update_requested: false,
            needs_full_update: true,
            swap_rb: false,
        }
    }
}

struct FrameBuffers {
    capture: Vec<u8>,
    scale: Vec<u8>,
    prev: Vec<u8>,
    dirty: Vec<DirtyRect>,
    wgc_hints: Vec<CaptureRect>,
    have_fresh_frame: bool,
    have_wgc_hints: bool,
}

impl FrameBuffers {
    fn new() -> Self {
        Self {
            capture: Vec::new(),
            scale: Vec::new(),
            prev: Vec::new(),
            dirty: Vec::with_capacity(2048),
            wgc_hints: Vec::with_capacity(64),
            have_fresh_frame: false,
            have_wgc_hints: false,
        }
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
        let _timer = timer_res::HighResTimer::acquire();

        let addr = format!("0.0.0.0:{}", self.config.port);
        let listener = TcpListener::bind(&addr)?;
        listener.set_nonblocking(true)?;

        self.print_startup_banner(&addr);

        while self.running.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, addr)) => self.accept_client(stream, addr),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(ACCEPT_SLEEP_MS));
                }
                Err(e) => {
                    warn!("Accept error: {}", e);
                    thread::sleep(Duration::from_millis(ACCEPT_SLEEP_MS));
                }
            }
        }

        println!("🛑 Server shut down");
        Ok(())
    }

    fn print_startup_banner(&self, addr: &str) {
        if self.config.password.is_some() {
            println!("🔐 Password authentication enabled");
        } else {
            println!("⚠️  No password set");
        }
        println!("🚀 VNC server listening on {}", addr);
        println!();
    }

    fn accept_client(&mut self, stream: TcpStream, addr: std::net::SocketAddr) {
        println!("✅ Client connected: {}", addr);
        if let Err(e) = self.handle_connection(stream) {
            println!("❌ Error: {}", e);
        }
        println!("👋 Client disconnected");
        println!("📊 Session stats:");
        self.stats.print_stats();
        println!("\n⏳ Waiting for next client...\n");
    }

    fn handle_connection(&mut self, stream: TcpStream) -> Result<()> {
        stream.set_nonblocking(false)?;
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECS)))?;
        stream.set_write_timeout(Some(Duration::from_secs(WRITE_TIMEOUT_SECS)))?;
        let _ = SockRef::from(&stream).set_send_buffer_size(4 * 1024 * 1024);
        let _ = SockRef::from(&stream).set_recv_buffer_size(256 * 1024);

        let reader_stream = stream.try_clone()?;
        let writer_stream = stream;

        let mut reader = BufReader::new(reader_stream.try_clone()?);
        let mut writer = BufWriter::with_capacity(WRITE_BUF_SIZE, writer_stream);

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
        let rx = spawn_reader_thread(reader_stream);

        let (scale_ctx, dst_stride, needs_scaling) = self.build_scale_context(eff_w, eff_h);
        let mut buffers = FrameBuffers::new();
        let mut state = SessionState::new();
        let frame_interval = Duration::from_millis(self.config.frame_interval_ms());
        let tile_size = self.config.tile_size;

        let mut last_send = Instant::now() - frame_interval;

        while self.running.load(Ordering::Relaxed) {
            if self.drain_messages(&rx, &mut state, eff_w, eff_h)? {
                return Ok(());
            }

            match self.capture.swap_frame(&mut buffers.capture) {
                Ok(true) => {
                    if needs_scaling {
                        if let Some(ref ctx) = scale_ctx {
                            ctx.scale_into(&buffers.capture, &mut buffers.scale);
                        }
                    }
                    buffers.have_fresh_frame = true;
                }
                Ok(false) => {}
                Err(e) => {
                    warn!("Capture error: {}", e);
                    thread::sleep(Duration::from_millis(CAPTURE_ERROR_SLEEP_MS));
                    continue;
                }
            }

            let can_send = state.update_requested
                && (buffers.have_fresh_frame || state.needs_full_update)
                && last_send.elapsed() >= frame_interval;

            if !can_send {
                if state.update_requested {
                    thread::sleep(Duration::from_micros(NO_FRAME_SLEEP_US));
                } else {
                    thread::sleep(Duration::from_micros(IDLE_SLEEP_US));
                }
                continue;
            }

            let frame: &[u8] = if needs_scaling {
                &buffers.scale
            } else {
                &buffers.capture
            };

            if state.needs_full_update {
                buffers.dirty.clear();
                buffers.dirty.push(DirtyRect::new(0, 0, eff_w, eff_h));
            } else if buffers.have_wgc_hints && !needs_scaling && !buffers.prev.is_empty() {
                wgc_hints_to_dirty(
                    &buffers.wgc_hints,
                    eff_w,
                    eff_h,
                    &mut buffers.dirty,
                );
            } else {
                find_dirty_regions(
                    frame,
                    &buffers.prev,
                    dst_stride,
                    eff_w,
                    eff_h,
                    tile_size,
                    &mut buffers.dirty,
                );
            }

            if buffers.dirty.is_empty() && !state.needs_full_update {
                buffers.have_fresh_frame = false;
                thread::sleep(Duration::from_micros(NO_FRAME_SLEEP_US));
                continue;
            }

            let dirty_count = buffers.dirty.len();
            let bytes_sent = match self.send_update(
                writer,
                frame,
                dst_stride,
                &buffers.dirty,
                state.swap_rb,
            )? {
                Some(n) => n,
                None => return Ok(()),
            };

            if needs_scaling {
                std::mem::swap(&mut buffers.scale, &mut buffers.prev);
            } else {
                std::mem::swap(&mut buffers.capture, &mut buffers.prev);
            }

            state.needs_full_update = false;
            state.update_requested = false;
            buffers.have_fresh_frame = false;
            last_send = Instant::now();

            self.stats.frame(bytes_sent, dirty_count);
        }

        Ok(())
    }

    fn build_scale_context(&self, eff_w: u16, eff_h: u16) -> (Option<ScaleContext>, usize, bool) {
        let src_w = self.capture.width() as usize;
        let src_h = self.capture.height() as usize;
        let src_stride = self.capture.stride();
        let needs_scaling = eff_w as usize != src_w || eff_h as usize != src_h;

        let ctx = if needs_scaling {
            Some(ScaleContext::new(
                src_w,
                src_h,
                src_stride,
                eff_w as usize,
                eff_h as usize,
            ))
        } else {
            None
        };

        let dst_stride = if needs_scaling {
            eff_w as usize * 4
        } else {
            src_stride
        };

        (ctx, dst_stride, needs_scaling)
    }

    fn drain_messages(
        &mut self,
        rx: &mpsc::Receiver<ReaderEvent>,
        state: &mut SessionState,
        eff_w: u16,
        eff_h: u16,
    ) -> Result<bool> {
        loop {
            match rx.try_recv() {
                Ok(ReaderEvent::Message(msg)) => self.handle_message(msg, state, eff_w, eff_h),
                Ok(ReaderEvent::Disconnected) => return Ok(true),
                Err(mpsc::TryRecvError::Empty) => return Ok(false),
                Err(mpsc::TryRecvError::Disconnected) => return Ok(true),
            }
        }
    }

    fn handle_message(
        &mut self,
        msg: ClientMessage,
        state: &mut SessionState,
        eff_w: u16,
        eff_h: u16,
    ) {
        match msg {
            ClientMessage::SetPixelFormat { format } => {
                let client_pf = PixelFormat::from_bytes(&format);
                state.swap_rb = client_pf.needs_bgr_swap();
                println!(
                    "🎨 Pixel format: r={} b={} (swap={})",
                    client_pf.red_shift, client_pf.blue_shift, state.swap_rb
                );
            }
            ClientMessage::SetEncodings { encodings } => {
                self.encoder.negotiate(&encodings);
                info!("Encodings: {:?}", encodings);
            }
            ClientMessage::FramebufferUpdateRequest { incremental, .. } => {
                state.update_requested = true;
                if !incremental {
                    state.needs_full_update = true;
                }
            }
            ClientMessage::PointerEvent { buttons, x, y } => {
                self.handle_pointer(buttons, x, y, state, eff_w, eff_h);
            }
            ClientMessage::KeyEvent { down, key } => {
                self.input.key_event(key, down);
            }
            ClientMessage::ClientCutText { .. } => {}
        }
    }

    fn handle_pointer(
        &mut self,
        buttons: u8,
        x: u16,
        y: u16,
        state: &mut SessionState,
        eff_w: u16,
        eff_h: u16,
    ) {
        let native_x = (x as u32 * self.capture.width() as u32 / eff_w.max(1) as u32) as u16;
        let native_y = (y as u32 * self.capture.height() as u32 / eff_h.max(1) as u32) as u16;
        self.input.move_mouse(native_x, native_y);

        let old = state.mouse_buttons;

        if pressed_edge(old, buttons, BTN_SCROLL_UP) {
            self.input.scroll(ScrollDirection::Up);
        }
        if pressed_edge(old, buttons, BTN_SCROLL_DOWN) {
            self.input.scroll(ScrollDirection::Down);
        }
        if pressed_edge(old, buttons, BTN_SCROLL_LEFT) {
            self.input.scroll(ScrollDirection::Left);
        }
        if pressed_edge(old, buttons, BTN_SCROLL_RIGHT) {
            self.input.scroll(ScrollDirection::Right);
        }

        let diff = old ^ buttons;
        if diff & BTN_LEFT != 0 {
            self.input.mouse_button(1, buttons & BTN_LEFT != 0);
        }
        if diff & BTN_MIDDLE != 0 {
            self.input.mouse_button(2, buttons & BTN_MIDDLE != 0);
        }
        if diff & BTN_RIGHT != 0 {
            self.input.mouse_button(3, buttons & BTN_RIGHT != 0);
        }

        state.mouse_buttons = buttons;
    }

    fn send_update<W: Write>(
        &mut self,
        writer: &mut W,
        frame: &[u8],
        stride: usize,
        dirty: &[DirtyRect],
        swap_rb: bool,
    ) -> Result<Option<usize>> {
        let mut bytes_sent: usize = 4;

        if dirty.is_empty() {
            if writer.write_all(&EMPTY_UPDATE).is_err() || writer.flush().is_err() {
                return Ok(None);
            }
            return Ok(Some(bytes_sent));
        }

        let mut header = [0u8; 4];
        header[2..4].copy_from_slice(&(dirty.len() as u16).to_be_bytes());
        if writer.write_all(&header).is_err() {
            return Ok(None);
        }

        let enc_id = self.encoder.encoding_id();

        for r in dirty {
            let mut rect_header = [0u8; 12];
            rect_header[0..2].copy_from_slice(&r.x.to_be_bytes());
            rect_header[2..4].copy_from_slice(&r.y.to_be_bytes());
            rect_header[4..6].copy_from_slice(&r.w.to_be_bytes());
            rect_header[6..8].copy_from_slice(&r.h.to_be_bytes());
            rect_header[8..12].copy_from_slice(&enc_id.to_be_bytes());

            if writer.write_all(&rect_header).is_err() {
                return Ok(None);
            }

            let data = self
                .encoder
                .encode_rect(frame, stride, r.x, r.y, r.w, r.h, swap_rb)?;
            bytes_sent += 12 + data.len();

            if writer.write_all(data).is_err() {
                return Ok(None);
            }
        }

        if writer.flush().is_err() {
            return Ok(None);
        }

        Ok(Some(bytes_sent))
    }
}

#[inline]
fn pressed_edge(old: u8, new: u8, mask: u8) -> bool {
    (new & mask) != 0 && (old & mask) == 0
}

fn wgc_hints_to_dirty(
    hints: &[CaptureRect],
    width: u16,
    height: u16,
    out: &mut Vec<DirtyRect>,
) {
    out.clear();
    if hints.is_empty() {
        return;
    }
    for r in hints {
        let cx = r.x.min(width);
        let cy = r.y.min(height);
        let cw = r.w.min(width.saturating_sub(cx));
        let ch = r.h.min(height.saturating_sub(cy));
        if cw > 0 && ch > 0 {
            out.push(DirtyRect::new(cx, cy, cw, ch));
        }
    }
}
