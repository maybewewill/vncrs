use super::ScreenCapture;
use crate::error::{Result, VncError};
use std::io::ErrorKind;
use std::thread;
use std::time::{Duration, Instant};

pub struct ScrapCapture {
    capturer: scrap::Capturer,
    width: u16,
    height: u16,
    stride: usize,
}

impl ScrapCapture {
    pub fn new() -> Result<Self> {
        let display = scrap::Display::primary().map_err(|e| VncError::Capture(e.to_string()))?;
        let w = display.width() as u16;
        let h = display.height() as u16;
        let capturer =
            scrap::Capturer::new(display).map_err(|e| VncError::Capture(e.to_string()))?;

        Ok(Self {
            capturer,
            width: w,
            height: h,
            stride: w as usize * 4,
        })
    }

    fn reinit(&mut self) -> Result<()> {
        let display = scrap::Display::primary().map_err(|e| VncError::Capture(e.to_string()))?;
        let w = display.width() as u16;
        let h = display.height() as u16;
        let capturer =
            scrap::Capturer::new(display).map_err(|e| VncError::Capture(e.to_string()))?;

        self.capturer = capturer;
        self.width = w;
        self.height = h;
        self.stride = w as usize * 4;

        Ok(())
    }
}

impl ScreenCapture for ScrapCapture {
    fn width(&self) -> u16 {
        self.width
    }

    fn height(&self) -> u16 {
        self.height
    }

    fn stride(&self) -> usize {
        self.stride
    }

    fn capture_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let start = Instant::now();
        let timeout = Duration::from_millis(50);

        loop {
            match self.capturer.frame() {
                Ok(frame) => {
                    self.stride = frame.len() / self.height as usize;
                    return Ok(Some(frame.to_vec()));
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    if start.elapsed() > timeout {
                        return Ok(None);
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Err(e) => {
                    log::warn!("Capture error ({}), reinitializing DXGI session...", e);

                    // Back off briefly before trying to reinit — the GPU/display
                    // driver may need a moment to settle (e.g. after unlock or
                    // display mode change).
                    thread::sleep(Duration::from_millis(500));

                    let mut attempt = 1u64;
                    loop {
                        match self.reinit() {
                            Ok(()) => {
                                log::info!("DXGI session reinitialized (attempt {})", attempt);
                                // Return None so the caller sends an empty update
                                // rather than stalling — the next call will capture
                                // a real frame.
                                return Ok(None);
                            }
                            Err(reinit_err) => {
                                let delay = Duration::from_millis((200 * attempt).min(10_000));
                                log::warn!(
                                    "Reinit attempt {} failed: {}. Retrying in {:?}...",
                                    attempt,
                                    reinit_err,
                                    delay
                                );
                                thread::sleep(delay);
                                attempt += 1;
                            }
                        }
                    }
                }
            }
        }
    }
}
