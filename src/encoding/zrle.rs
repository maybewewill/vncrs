use super::Encoder;
use crate::error::Result;
use flate2::{Compress, Compression, FlushCompress};

const ZRLE_TILE: usize = 64;
const MAX_PALETTE: usize = 16;

pub struct ZrleEncoder {
    compress: Compress,
    uncompressed: Vec<u8>,
    // Stack-allocated palette — no HashMap, no heap
    pal_keys: [u32; MAX_PALETTE],
    pal_cpixels: [[u8; 3]; MAX_PALETTE],
    pal_count: usize,
}

impl ZrleEncoder {
    pub fn new() -> Self {
        Self {
            compress: Compress::new(Compression::fast(), true),
            uncompressed: Vec::with_capacity(256 * 1024),
            pal_keys: [0; MAX_PALETTE],
            pal_cpixels: [[0; 3]; MAX_PALETTE],
            pal_count: 0,
        }
    }

    // ── CPIXEL: 3 bytes from BGRA, optionally swapping R↔B ────

    #[inline(always)]
    fn cpixel_at(pixels: &[u8], off: usize, swap_rb: bool) -> ([u8; 3], u32) {
        let cp = if swap_rb {
            [pixels[off + 2], pixels[off + 1], pixels[off]]
        } else {
            [pixels[off], pixels[off + 1], pixels[off + 2]]
        };
        let key = (cp[0] as u32) | ((cp[1] as u32) << 8) | ((cp[2] as u32) << 16);
        (cp, key)
    }

    // ── palette analysis: single pass, inline array ────────────
    //
    // For solid tiles (1 color):  4096 × 1 compare  = ~8μs
    // For 2-4 color tiles:        4096 × 1-2 compare = ~12μs
    // For complex tiles (>16):    bails after ~20-30 pixels = ~0.1μs
    //
    // vs HashMap v1:              4096 × hash+probe  = ~200μs

    fn analyze_tile(
        &mut self,
        pixels: &[u8],
        stride: usize,
        tx: usize,
        ty: usize,
        tw: usize,
        th: usize,
        swap_rb: bool,
    ) -> bool {
        self.pal_count = 0;

        for row in ty..ty + th {
            let base = row * stride + tx * 4;
            for col in 0..tw {
                let off = base + col * 4;
                let (cp, key) = Self::cpixel_at(pixels, off, swap_rb);

                let mut found = false;
                for i in 0..self.pal_count {
                    if self.pal_keys[i] == key {
                        found = true;
                        break;
                    }
                }

                if !found {
                    if self.pal_count >= MAX_PALETTE {
                        return false; // >16 colors → raw
                    }
                    self.pal_keys[self.pal_count] = key;
                    self.pal_cpixels[self.pal_count] = cp;
                    self.pal_count += 1;
                }
            }
        }
        true
    }

    #[inline]
    fn pal_idx(&self, key: u32) -> u8 {
        for i in 0..self.pal_count {
            if self.pal_keys[i] == key {
                return i as u8;
            }
        }
        0
    }

    // ── tile dispatch ──────────────────────────────────────────

    fn encode_tile(
        &mut self,
        pixels: &[u8],
        stride: usize,
        tx: usize,
        ty: usize,
        tw: usize,
        th: usize,
        swap_rb: bool,
    ) {
        // Bounds guard
        let last_byte = (ty + th - 1) * stride + (tx + tw) * 4;
        if last_byte > pixels.len() {
            self.uncompressed.push(1);
            self.uncompressed.extend_from_slice(&[0, 0, 0]);
            return;
        }

        let small_palette = self.analyze_tile(pixels, stride, tx, ty, tw, th, swap_rb);

        if small_palette && self.pal_count <= 1 {
            // ── solid (sub-encoding 1) ─────────────────────
            let cp = if self.pal_count == 1 {
                self.pal_cpixels[0]
            } else {
                [0, 0, 0]
            };
            self.uncompressed.push(1);
            self.uncompressed.extend_from_slice(&cp);
        } else if small_palette {
            // ── packed palette (sub-encoding 2-16) ─────────
            self.encode_packed(pixels, stride, tx, ty, tw, th, swap_rb);
        } else {
            // ── raw CPIXELs (sub-encoding 0) ───────────────
            self.encode_raw(pixels, stride, tx, ty, tw, th, swap_rb);
        }
    }

    // ── packed palette: N cpixels + bit-packed indices ─────────
    //
    //   2 colors  → 1 bit/pixel
    //   3-4       → 2 bits/pixel
    //   5-16      → 4 bits/pixel
    //
    // MSB-first packing, padded to byte boundary per row.

    fn encode_packed(
        &mut self,
        pixels: &[u8],
        stride: usize,
        tx: usize,
        ty: usize,
        tw: usize,
        th: usize,
        swap_rb: bool,
    ) {
        let nc = self.pal_count;
        let bpp: usize = if nc <= 2 { 1 } else if nc <= 4 { 2 } else { 4 };

        // Header: palette size = sub-encoding type
        self.uncompressed.push(nc as u8);

        // Palette entries
        for i in 0..nc {
            self.uncompressed.extend_from_slice(&self.pal_cpixels[i]);
        }

        // Bit-packed pixel indices
        let mask: u8 = (1 << bpp) - 1;

        for row in ty..ty + th {
            let base = row * stride + tx * 4;
            let mut byte = 0u8;
            let mut bits = 0usize;

            for col in 0..tw {
                let off = base + col * 4;
                let (_, key) = Self::cpixel_at(pixels, off, swap_rb);
                let idx = self.pal_idx(key);

                byte = (byte << bpp) | (idx & mask);
                bits += bpp;

                if bits >= 8 {
                    self.uncompressed.push(byte);
                    byte = 0;
                    bits = 0;
                }
            }

            // Row padding
            if bits > 0 {
                byte <<= 8 - bits;
                self.uncompressed.push(byte);
            }
        }
    }

    // ── raw CPIXELs: bulk 3-byte extraction ────────────────────
    //
    // Pre-allocate exact size, direct writes — no per-pixel push/extend.
    // Branch (swap_rb) is OUTSIDE the inner loop → compiler vectorises.

    fn encode_raw(
        &mut self,
        pixels: &[u8],
        stride: usize,
        tx: usize,
        ty: usize,
        tw: usize,
        th: usize,
        swap_rb: bool,
    ) {
        self.uncompressed.push(0); // sub-encoding 0 = raw

        let start = self.uncompressed.len();
        let total = tw * th * 3;
        self.uncompressed.resize(start + total, 0);
        let dst = &mut self.uncompressed[start..start + total];

        let mut di = 0;
        if swap_rb {
            for row in ty..ty + th {
                let base = row * stride + tx * 4;
                for col in 0..tw {
                    let off = base + col * 4;
                    dst[di] = pixels[off + 2];
                    dst[di + 1] = pixels[off + 1];
                    dst[di + 2] = pixels[off];
                    di += 3;
                }
            }
        } else {
            for row in ty..ty + th {
                let base = row * stride + tx * 4;
                for col in 0..tw {
                    let off = base + col * 4;
                    dst[di] = pixels[off];
                    dst[di + 1] = pixels[off + 1];
                    dst[di + 2] = pixels[off + 2];
                    di += 3;
                }
            }
        }
    }
}

// ── Encoder trait ──────────────────────────────────────────────

impl Encoder for ZrleEncoder {
    fn encoding_id(&self) -> i32 {
        16
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
        let rx = x as usize;
        let ry = y as usize;
        let rw = w as usize;
        let rh = h as usize;

        if rw == 0 || rh == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            return Ok(());
        }

        // Encode 64×64 tiles into uncompressed buffer
        self.uncompressed.clear();

        for ty_off in (0..rh).step_by(ZRLE_TILE) {
            for tx_off in (0..rw).step_by(ZRLE_TILE) {
                let tw = ZRLE_TILE.min(rw - tx_off);
                let th = ZRLE_TILE.min(rh - ty_off);
                self.encode_tile(
                    pixels, stride,
                    rx + tx_off, ry + ty_off,
                    tw, th, swap_rb,
                );
            }
        }

        // 4-byte length prefix (patched after compression)
        let len_pos = out.len();
        out.extend_from_slice(&[0, 0, 0, 0]);

        // Compress with persistent zlib stream (shared across all rects)
        let data_start = out.len();
        self.compress
            .compress_vec(&self.uncompressed, out, FlushCompress::Sync)
            .map_err(|e| crate::error::VncError::Encoding(e.to_string()))?;
        let compressed_len = out.len() - data_start;

        // Patch length
        out[len_pos..len_pos + 4]
            .copy_from_slice(&(compressed_len as u32).to_be_bytes());

        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_frame(w: usize, h: usize, color: [u8; 4]) -> (Vec<u8>, usize) {
        let stride = w * 4;
        let mut buf = vec![0u8; stride * h];
        for pixel in buf.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
        (buf, stride)
    }

    #[test]
    fn test_solid_tiny() {
        let mut enc = ZrleEncoder::new();
        let (frame, stride) = solid_frame(64, 64, [100, 150, 200, 255]);
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 64, 64, false, &mut out)
            .unwrap();
        assert!(out.len() < 30, "Solid 64x64 = {} bytes", out.len());
    }

    #[test]
    fn test_zrle_beats_raw_for_solid() {
        let mut zrle = ZrleEncoder::new();
        let mut raw = super::super::raw::RawEncoder;
        let (frame, stride) = solid_frame(128, 128, [50, 100, 150, 255]);

        let mut z_out = Vec::new();
        zrle.encode_rect_into(&frame, stride, 0, 0, 128, 128, false, &mut z_out).unwrap();
        let mut r_out = Vec::new();
        raw.encode_rect_into(&frame, stride, 0, 0, 128, 128, false, &mut r_out).unwrap();

        assert!(z_out.len() < r_out.len() / 10);
    }

    #[test]
    fn test_two_color_compact() {
        let mut enc = ZrleEncoder::new();
        let w = 64usize;
        let h = 64usize;
        let stride = w * 4;
        let mut frame = vec![0u8; stride * h];
        for y in 0..h {
            let c = if y < h / 2 { [255, 0, 0, 255] } else { [0, 0, 255, 255] };
            for x in 0..w {
                let off = y * stride + x * 4;
                frame[off..off + 4].copy_from_slice(&c);
            }
        }
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, w as u16, h as u16, false, &mut out)
            .unwrap();
        // 2-color packed: ~512 bytes + overhead, after zlib should be small
        assert!(out.len() < 200, "Two-color = {} bytes", out.len());
    }

    #[test]
    fn test_random_doesnt_crash() {
        let mut enc = ZrleEncoder::new();
        let w = 100usize;
        let h = 80usize;
        let stride = w * 4;
        let mut frame = vec![0u8; stride * h];
        for (i, b) in frame.iter_mut().enumerate() {
            *b = ((i * 7 + 13) % 256) as u8;
        }
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, w as u16, h as u16, false, &mut out)
            .unwrap();
        assert!(!out.is_empty());
    }

    #[test]
    fn test_subregion() {
        let mut enc = ZrleEncoder::new();
        let (frame, stride) = solid_frame(200, 200, [42, 42, 42, 255]);
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 10, 10, 100, 80, false, &mut out).unwrap();
        assert!(out.len() < 100);
    }

    #[test]
    fn test_1x1_tile() {
        let mut enc = ZrleEncoder::new();
        let (frame, stride) = solid_frame(1, 1, [255, 128, 64, 255]);
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 1, 1, false, &mut out).unwrap();
        assert!(!out.is_empty());
    }

    #[test]
    fn test_swap_rb() {
        let mut enc = ZrleEncoder::new();
        let (frame, stride) = solid_frame(1, 1, [10, 20, 30, 255]); // B=10, G=20, R=30
        let mut out_no = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 1, 1, false, &mut out_no).unwrap();
        let mut enc2 = ZrleEncoder::new();
        let mut out_yes = Vec::new();
        enc2.encode_rect_into(&frame, stride, 0, 0, 1, 1, true, &mut out_yes).unwrap();
        // Outputs should differ (different cpixel byte order)
        assert_ne!(out_no, out_yes);
    }
}
