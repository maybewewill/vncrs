pub mod hextile;
pub mod raw;
pub mod zlib;
pub mod zrle;

use crate::error::Result;

pub trait Encoder {
    fn encoding_id(&self) -> i32;
    fn encode_rect_into(
        &mut self,
        pixels: &[u8],
        stride: usize,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        swap_rb: bool,
        out: &mut Vec<u8>,
    ) -> Result<()>;
}

pub struct EncoderSet {
    raw: raw::RawEncoder,
    hextile: hextile::HextileEncoder,
    zlib: zlib::ZlibCompressor,
    zrle: zrle::ZrleEncoder,
    active: i32,
    buf: Vec<u8>,
}

impl EncoderSet {
    pub fn new() -> Self {
        Self {
            raw: raw::RawEncoder,
            hextile: hextile::HextileEncoder::new(),
            zlib: zlib::ZlibCompressor::new(),
            zrle: zrle::ZrleEncoder::new(),
            active: 0,
            buf: Vec::with_capacity(256 * 1024),
        }
    }

    pub fn negotiate(&mut self, client_encodings: &[i32]) {
        for &enc in client_encodings {
            match enc {
                5 | 6 | 0 => {
                    let name = match enc {
                        5 => "Hextile",
                        6 => "Zlib",
                        0 => "Raw",
                        _ => unreachable!(),
                    };
                    println!("🎨 Negotiated encoding: {}", name);
                    self.active = enc;
                    return;
                }
                _ => continue,
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
    ) -> Result<&[u8]> {
        self.buf.clear();
        match self.active {
            16 => self
                .zrle
                .encode_rect_into(pixels, stride, x, y, w, h, swap_rb, &mut self.buf)?,
            5 => {
                self.hextile
                    .encode_rect_into(pixels, stride, x, y, w, h, swap_rb, &mut self.buf)?
            }
            6 => self
                .zlib
                .encode_rect_into(pixels, stride, x, y, w, h, swap_rb, &mut self.buf)?,
            _ => self
                .raw
                .encode_rect_into(pixels, stride, x, y, w, h, swap_rb, &mut self.buf)?,
        }
        Ok(&self.buf)
    }
}
