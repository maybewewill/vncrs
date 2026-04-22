use super::{CaptureRect, ScreenCapture};
use crate::error::{Result, VncError};
use arc_swap::ArcSwap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
};

#[derive(Default)]
struct FramePayload {
    data: Vec<u8>,
    dirty: Vec<CaptureRect>,
    had_dirty_api: bool,
    generation: u64,
}

struct CaptureState {
    slot: ArcSwap<Option<FramePayload>>,
    active: AtomicBool,
}

struct Handler {
    state: Arc<CaptureState>,
    staging: Vec<u8>,
    dirty_staging: Vec<CaptureRect>,
    width: u16,
    height: u16,
    generation: u64,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = HandlerFlags;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            state: ctx.flags.state,
            staging: Vec::new(),
            dirty_staging: Vec::with_capacity(64),
            width: ctx.flags.width,
            height: ctx.flags.height,
            generation: 0,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> std::result::Result<(), Self::Error> {
        if !self.state.active.load(Ordering::Relaxed) {
            capture_control.stop();
            return Ok(());
        }

        self.dirty_staging.clear();
        let had_dirty_api = match frame.dirty_regions() {
            Ok(regions) => {
                for r in regions {
                    let x = r.x.max(0) as u32;
                    let y = r.y.max(0) as u32;
                    let w = r.width.max(0) as u32;
                    let h = r.height.max(0) as u32;
                    if w == 0 || h == 0 {
                        continue;
                    }
                    let cx = x.min(self.width as u32) as u16;
                    let cy = y.min(self.height as u32) as u16;
                    let cw = w.min(self.width as u32 - cx as u32) as u16;
                    let ch = h.min(self.height as u32 - cy as u32) as u16;
                    if cw > 0 && ch > 0 {
                        self.dirty_staging.push(CaptureRect { x: cx, y: cy, w: cw, h: ch });
                    }
                }
                true
            }
            Err(_) => false,
        };

        let mut buffer = frame.buffer()?;
        let raw = buffer.as_raw_buffer();

        self.staging.resize(raw.len(), 0);
        self.staging.copy_from_slice(raw);

        self.generation = self.generation.wrapping_add(1);
        let payload = FramePayload {
            data: std::mem::take(&mut self.staging),
            dirty: std::mem::take(&mut self.dirty_staging),
            had_dirty_api,
            generation: self.generation,
        };
        self.staging.reserve(payload.data.capacity());
        self.dirty_staging.reserve(64);

        let old = self.state.slot.swap(Arc::new(Some(payload)));
        if let Some(mut prev) = Arc::try_unwrap(old).ok().and_then(|o| o) {
            if prev.data.capacity() > self.staging.capacity() {
                prev.data.clear();
                self.staging = prev.data;
            }
        }

        Ok(())
    }

    fn on_closed(&mut self) -> std::result::Result<(), Self::Error> {
        log::info!("Capture session closed.");
        self.state.active.store(false, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Clone)]
struct HandlerFlags {
    state: Arc<CaptureState>,
    width: u16,
    height: u16,
}

pub struct WindowsCapture {
    state: Arc<CaptureState>,
    width: u16,
    height: u16,
    stride: usize,
    last_generation: u64,
    last_dirty: Vec<CaptureRect>,
    last_had_dirty_api: bool,
    _capture_thread: std::thread::JoinHandle<()>,
}

impl WindowsCapture {
    pub fn new() -> Result<Self> {
        let monitor = Monitor::primary().map_err(|e| VncError::Capture(e.to_string()))?;
        let width = monitor
            .width()
            .map_err(|e| VncError::Capture(e.to_string()))? as u16;
        let height = monitor
            .height()
            .map_err(|e| VncError::Capture(e.to_string()))? as u16;
        let stride = width as usize * 4;

        let state = Arc::new(CaptureState {
            slot: ArcSwap::from_pointee(None),
            active: AtomicBool::new(true),
        });

        let thread_state = state.clone();
        let flags = HandlerFlags {
            state: thread_state.clone(),
            width,
            height,
        };

        let capture_thread = std::thread::spawn(move || {
            let settings = Settings::new(
                monitor,
                CursorCaptureSettings::WithoutCursor,
                DrawBorderSettings::WithoutBorder,
                SecondaryWindowSettings::Default,
                MinimumUpdateIntervalSettings::Default,
                DirtyRegionSettings::ReportOnly,
                ColorFormat::Bgra8,
                flags,
            );

            if let Err(e) = Handler::start_free_threaded(settings) {
                log::error!("Capture error: {}", e);
                thread_state.active.store(false, Ordering::Relaxed);
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(200));

        if !state.active.load(Ordering::Relaxed) {
            return Err(VncError::Capture("Capture failed to start".into()));
        }

        Ok(Self {
            state,
            width,
            height,
            stride,
            last_generation: 0,
            last_dirty: Vec::with_capacity(64),
            last_had_dirty_api: false,
            _capture_thread: capture_thread,
        })
    }
}

impl Drop for WindowsCapture {
    fn drop(&mut self) {
        self.state.active.store(false, Ordering::Relaxed);
    }
}

impl ScreenCapture for WindowsCapture {
    fn width(&self) -> u16 {
        self.width
    }
    fn height(&self) -> u16 {
        self.height
    }
    fn stride(&self) -> usize {
        self.stride
    }

    fn swap_frame(&mut self, buf: &mut Vec<u8>) -> Result<bool> {
        if !self.state.active.load(Ordering::Relaxed) {
            return Err(VncError::Capture("Capture session ended".into()));
        }
        let guard = self.state.slot.load();
        let payload = match guard.as_ref().as_ref() {
            Some(p) => p,
            None => return Ok(false),
        };
        if payload.generation == self.last_generation {
            return Ok(false);
        }
        self.last_generation = payload.generation;

        buf.clear();
        buf.extend_from_slice(&payload.data);

        self.last_dirty.clear();
        self.last_dirty.extend_from_slice(&payload.dirty);
        self.last_had_dirty_api = payload.had_dirty_api;

        Ok(true)
    }

    fn take_dirty_hints(&mut self, out: &mut Vec<CaptureRect>) -> bool {
        out.clear();
        if !self.last_had_dirty_api {
            return false;
        }
        out.extend_from_slice(&self.last_dirty);
        true
    }
}
