use super::Encoder;
use crate::error::Result;

pub struct HextileEncoder;

const RAW: u8 = 1;
const BACKGROUND_SPECIFIED: u8 = 2;

impl HextileEncoder {
    pub fn new() -> Self {
        Self
    }
    #[inline]
    fn is_solid(
        pixels: &[u8],
        stride: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) -> Option<[u8; 4]> {
        let first_offset = y * stride + x * 4;
        if first_offset + 4 > pixels.len() {
            return None;
        }
        let color = [
            pixels[first_offset],
            pixels[first_offset + 1],
            pixels[first_offset + 2],
            pixels[first_offset + 3],
        ];

        for row in y..y + h {
            for col in x..x + w {
                let offset = row * stride + col * 4;
                if pixels[offset] != color[0]
                    || pixels[offset + 1] != color[1]
                    || pixels[offset + 2] != color[2]
                    || pixels[offset + 3] != color[3]
                {
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
        let mut data = Vec::new();

        let rx = x as usize;
        let ry = y as usize;
        let rw = w as usize;
        let rh = h as usize;

        for ty in (0..rh).step_by(16) {
            for tx in (0..rw).step_by(16) {
                let tw = 16.min(rw - tx);
                let th = 16.min(rh - ty);
                let abs_x = rx + tx;
                let abs_y = ry + ty;

                if let Some(mut color) = Self::is_solid(pixels, stride, abs_x, abs_y, tw, th) {
                    if swap_rb {
                        color.swap(0, 2);
                    }
                    data.push(BACKGROUND_SPECIFIED);
                    data.extend_from_slice(&color);
                } else {
                    data.push(RAW);

                    for row in abs_y..abs_y + th {
                        let start = row * stride + abs_x * 4;
                        let end = start + tw * 4;
                        if swap_rb {
                            let mut chunk = pixels[start..end].to_vec();
                            for pixel in chunk.chunks_exact_mut(4) {
                                pixel.swap(0, 2);
                            }
                            data.extend_from_slice(&chunk);
                        } else {
                            data.extend_from_slice(&pixels[start..end]);
                        }
                    }
                }
            }
        }

        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::HextileEncoder;
    use crate::encoding::raw::RawEncoder;
    use crate::encoding::Encoder;

    const RAW: u8 = 0x01;
    const BG: u8 = 0x02;

    #[test]
    fn test_solid_tile() {
        let mut enc = HextileEncoder;
        let stride = 16 * 4;
        let frame = vec![100u8; stride * 16];

        let data = enc
            .encode_rect(&frame, stride, 0, 0, 16, 16, false)
            .unwrap();
        assert_eq!(data[0], BG);
        assert_eq!(data.len(), 5);
    }

    #[test]
    fn test_solid_tile_repeat_same_color() {
        let mut enc = HextileEncoder;
        let stride = 32 * 4;
        let frame = vec![55u8; stride * 16];

        let data = enc
            .encode_rect(&frame, stride, 0, 0, 32, 16, false)
            .unwrap();
        assert_eq!(data[0], BG);
        assert_eq!(data[5], 0);
        assert_eq!(data.len(), 5 + 1);
    }

    #[test]
    fn test_raw_fallback() {
        let mut enc = HextileEncoder;
        let stride = 16 * 4;
        let mut frame = vec![0u8; stride * 16];
        for (i, byte) in frame.iter_mut().enumerate() {
            *byte = (i % 256) as u8;
        }

        let data = enc
            .encode_rect(&frame, stride, 0, 0, 16, 16, false)
            .unwrap();
        assert_eq!(data[0], RAW);
        assert_eq!(data.len(), 1 + 16 * 16 * 4);
    }

    #[test]
    fn test_hextile_smaller_than_raw_for_solid() {
        let mut hextile = HextileEncoder;
        let mut raw = RawEncoder;

        let stride = 64 * 4;
        let frame = vec![128u8; stride * 64];

        let hex_data = hextile
            .encode_rect(&frame, stride, 0, 0, 64, 64, false)
            .unwrap();
        let raw_data = raw
            .encode_rect(&frame, stride, 0, 0, 64, 64, false)
            .unwrap();
        assert!(hex_data.len() < raw_data.len());
    }
}
