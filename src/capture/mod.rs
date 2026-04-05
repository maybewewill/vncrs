pub mod scrap;

use crate::error::Result;

pub trait ScreenCapture {
    fn width(&self) -> u16;
    fn height(&self) -> u16;
    fn stride(&self) -> usize;
    fn capture_frame(&mut self) -> Result<Option<Vec<u8>>>;
}
