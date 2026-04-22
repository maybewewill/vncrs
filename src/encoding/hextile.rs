use super::raw::swap_rb_inplace;
use super::Encoder;
use crate::error::Result;

const RAW: u8 = 1;
const BACKGROUND_SPECIFIED: u8 = 2;

pub struct HextileEncoder {
    // Adaptive: if too many raw tiles, switch to raw encoding for next rect
    raw_tile_count: u32,
    solid_tile_count: u32,
}

impl HextileEncoder {
    pub fn new() -> Self {
        Self {
            raw_tile_count: 0,
            solid_tile_count: 0,
        }
    }

    /// u32 comparison — 1 instruction per pixel instead of 4 byte compares
    #[inline]
    fn is_solid(
        pixels: &[u8],
        stride: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) -> Option<[u8; 4]> {
        let first = y * stride + x * 4;
        if first + 4 > pixels.len() {
            return None;
        }

        let color = [
            pixels[first],
            pixels[first + 1],
            pixels[first + 2],
            pixels[first + 3],
        ];
        let color_u32 = u32::from_ne_bytes(color);
        let row_bytes = w * 4;

        for row in y..y + h {
            let rs = row * stride + x * 4;
            let re = rs + row_bytes;
            if re > pixels.len() {
                return None;
            }
            // Compare 4 bytes at a time
            for chunk in pixels[rs..re].chunks_exact(4) {
                if u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) != color_u32 {
                    return None;
                }
            }
        }
        Some(color)
    }
}

impl Encoder for HextileEncoder {
    fn encoding_id(&self) -> i32 {
        5
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
        let mut current_bg: Option<[u8; 4]> = None;

        self.raw_tile_count = 0;
        self.solid_tile_count = 0;

        for ty in (0..rh).step_by(16) {
            for tx in (0..rw).step_by(16) {
                let tw = 16.min(rw - tx);
                let th = 16.min(rh - ty);
                let abs_x = rx + tx;
                let abs_y = ry + ty;

                if let Some(mut color) = Self::is_solid(pixels, stride, abs_x, abs_y, tw, th) {
                    self.solid_tile_count += 1;
                    if swap_rb {
                        color.swap(0, 2);
                    }
                    if Some(color) == current_bg {
                        out.push(0);
                    } else {
                        out.push(BACKGROUND_SPECIFIED);
                        out.extend_from_slice(&color);
                        current_bg = Some(color);
                    }
                } else {
                    self.raw_tile_count += 1;
                    out.push(RAW);
                    let mark = out.len();
                    let tile_bytes = tw * 4;

                    // Copy all tile rows into out — no intermediate alloc
                    for row in abs_y..abs_y + th {
                        let start = row * stride + abs_x * 4;
                        out.extend_from_slice(&pixels[start..start + tile_bytes]);
                    }

                    // Single in-place swap over entire tile
                    if swap_rb {
                        swap_rb_inplace(&mut out[mark..]);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BG: u8 = 0x02;

    #[test]
    fn test_solid_tile() {
        let mut enc = HextileEncoder::new();
        let stride = 16 * 4;
        let frame = vec![100u8; stride * 16];
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 16, 16, false, &mut out)
            .unwrap();
        assert_eq!(out[0], BG);
        assert_eq!(out.len(), 5);
    }

    #[test]
    fn test_solid_tile_repeat_same_color() {
        let mut enc = HextileEncoder::new();
        let stride = 32 * 4;
        let frame = vec![55u8; stride * 16];
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 32, 16, false, &mut out)
            .unwrap();
        assert_eq!(out[0], BG);
        assert_eq!(out[5], 0);
        assert_eq!(out.len(), 5 + 1);
    }

    #[test]
    fn test_raw_fallback() {
        let mut enc = HextileEncoder::new();
        let stride = 16 * 4;
        let mut frame = vec![0u8; stride * 16];
        for (i, byte) in frame.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 16, 16, false, &mut out)
            .unwrap();
        assert_eq!(out[0], RAW);
        assert_eq!(out.len(), 1 + 16 * 16 * 4);
    }

    #[test]
    fn test_hextile_smaller_than_raw_for_solid() {
        let mut hextile = HextileEncoder::new();
        let mut raw = super::super::raw::RawEncoder;

        let stride = 64 * 4;
        let frame = vec![128u8; stride * 64];

        let mut hex_out = Vec::new();
        hextile
            .encode_rect_into(&frame, stride, 0, 0, 64, 64, false, &mut hex_out)
            .unwrap();
        let mut raw_out = Vec::new();
        raw.encode_rect_into(&frame, stride, 0, 0, 64, 64, false, &mut raw_out)
            .unwrap();

        assert!(hex_out.len() < raw_out.len());
    }
}
