use super::Encoder;
use crate::error::Result;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

pub struct ZlibCompressor {
    compression: Compression,
}

impl ZlibCompressor {
    pub fn new() -> Self {
        Self {
            compression: Compression::fast(),
        }
    }
}

impl Encoder for ZlibCompressor {
    fn encoding_id(&self) -> i32 {
        6
    }

    fn encode_rect(
        &mut self,
        pixels: &[u8],
        stride: usize,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        swap_rb: bool,
    ) -> Result<Vec<u8>> {
        let row_bytes = w as usize * 4;
        let mut raw = Vec::with_capacity(row_bytes * h as usize);

        for row in y..y + h {
            let start = row as usize * stride + x as usize * 4;
            let end = start + row_bytes;
            raw.extend_from_slice(&pixels[start..end]);
        }

        if swap_rb {
            for pixel in raw.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        let mut encoder = ZlibEncoder::new(Vec::new(), self.compression);
        encoder.write_all(&raw)?;
        let compressed = encoder.finish()?;

        let mut result = Vec::with_capacity(4 + compressed.len());
        result.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        result.extend_from_slice(&compressed);

        Ok(result)
    }
}
