pub mod windows;

use crate::error::Result;

#[derive(Copy, Clone, Debug)]
pub struct CaptureRect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

pub trait ScreenCapture {
    fn width(&self) -> u16;
    fn height(&self) -> u16;
    fn stride(&self) -> usize;

    fn swap_frame(&mut self, buf: &mut Vec<u8>) -> Result<bool>;

    fn take_dirty_hints(&mut self, _out: &mut Vec<CaptureRect>) -> bool {
        false
    }
}
