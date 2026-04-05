pub mod hextile;
pub mod raw;
pub mod zlib;

use crate::error::Result;

pub trait Encoder {
    fn encoding_id(&self) -> i32;
    fn encode_rect(
        &mut self,
        pixels: &[u8],
        stride: usize,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        swap_rb: bool,
    ) -> Result<Vec<u8>>;
}

pub struct EncoderSet {
    raw: raw::RawEncoder,
    hextile: hextile::HextileEncoder,
    zlib: zlib::ZlibCompressor,
    active: i32,
}

impl EncoderSet {
    pub fn new() -> Self {
        Self {
            raw: raw::RawEncoder,
            hextile: hextile::HextileEncoder,
            zlib: zlib::ZlibCompressor::new(),
            active: 0,
        }
    }

    pub fn negotiate(&mut self, client_encodings: &[i32]) {
        let our_priority = [5, 6, 0];

        for &our_enc in &our_priority {
            if client_encodings.contains(&our_enc) {
                let name = match our_enc {
                    0 => "Raw",
                    5 => "Hextile",
                    6 => "Zlib",
                    _ => "Unknown",
                };
                println!("🎨 Negotiated encoding: {}", name);
                self.active = our_enc;
                return;
            }
        }
        println!("🎨 Negotiated encoding: Raw (fallback)");
        self.active = 0;
    }

    pub fn encoding_id(&self) -> i32 {
        self.active
    }

    pub fn encode_rect(
        &mut self,
        pixels: &[u8],
        stride: usize,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        swap_rb: bool,
    ) -> Result<Vec<u8>> {
        match self.active {
            5 => self
                .hextile
                .encode_rect(pixels, stride, x, y, w, h, swap_rb),
            6 => self.zlib.encode_rect(pixels, stride, x, y, w, h, swap_rb),
            _ => self.raw.encode_rect(pixels, stride, x, y, w, h, swap_rb),
        }
    }
}
