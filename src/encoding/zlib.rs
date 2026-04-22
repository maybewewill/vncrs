use super::raw::swap_rb_inplace;
use super::Encoder;
use crate::error::Result;
use flate2::{Compress, Compression, FlushCompress};

pub struct ZlibCompressor {
    compress: Compress,
    raw_buf: Vec<u8>,
}

impl ZlibCompressor {
    pub fn new() -> Self {
        Self {
            compress: Compress::new(Compression::fast(), true),
            raw_buf: Vec::with_capacity(64 * 1024),
        }
    }
}

impl Encoder for ZlibCompressor {
    fn encoding_id(&self) -> i32 {
        6
    }

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
    ) -> Result<()> {
        let row_bytes = w as usize * 4;
        let total = row_bytes * h as usize;

        self.raw_buf.clear();
        self.raw_buf.reserve(total);

        if x == 0 && row_bytes == stride {
            let start = y as usize * stride;
            self.raw_buf
                .extend_from_slice(&pixels[start..start + total]);
        } else {
            for row in y..y + h {
                let start = row as usize * stride + x as usize * 4;
                self.raw_buf
                    .extend_from_slice(&pixels[start..start + row_bytes]);
            }
        }

        if swap_rb {
            swap_rb_inplace(&mut self.raw_buf);
        }

        let len_pos = out.len();
        out.extend_from_slice(&[0, 0, 0, 0]);

        let before = self.compress.total_out();
        self.compress
            .compress_vec(&self.raw_buf, out, FlushCompress::Partial)
            .map_err(|e| crate::error::VncError::Encoding(e.to_string()))?;
        self.compress
            .compress_vec(&[], out, FlushCompress::Sync)
            .map_err(|e| crate::error::VncError::Encoding(e.to_string()))?;
        let compressed_len = (self.compress.total_out() - before) as usize;

        out.truncate(len_pos + 4 + compressed_len);

        out[len_pos..len_pos + 4].copy_from_slice(&(compressed_len as u32).to_be_bytes());

        Ok(())
    }
}
