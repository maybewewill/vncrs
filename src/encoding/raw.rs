use super::Encoder;
use crate::error::Result;

pub struct RawEncoder;

/// Swap R↔B in-place. Compiler auto-vectorises this to SIMD on x86_64.
#[inline]
pub(crate) fn swap_rb_inplace(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

impl Encoder for RawEncoder {
    fn encoding_id(&self) -> i32 {
        0
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
        out.reserve(total);
        let mark = out.len();

        if x == 0 && row_bytes == stride {
            let start = y as usize * stride;
            out.extend_from_slice(&pixels[start..start + total]);
        } else {
            for row in y..y + h {
                let start = row as usize * stride + x as usize * 4;
                out.extend_from_slice(&pixels[start..start + row_bytes]);
            }
        }

        if swap_rb {
            swap_rb_inplace(&mut out[mark..]);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_frame(width: usize, height: usize) -> (Vec<u8>, usize) {
        let stride = width * 4;
        let mut frame = vec![0u8; stride * height];
        for y in 0..height.min(10) {
            for x in 0..width.min(10) {
                let off = y * stride + x * 4;
                frame[off] = 0;
                frame[off + 1] = 0;
                frame[off + 2] = 255;
                frame[off + 3] = 255;
            }
        }
        (frame, stride)
    }

    #[test]
    fn test_raw_encode_size() {
        let mut enc = RawEncoder;
        let (frame, stride) = make_test_frame(100, 100);
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 10, 10, false, &mut out)
            .unwrap();
        assert_eq!(out.len(), 10 * 10 * 4);
    }

    #[test]
    fn test_raw_encode_with_swap() {
        let mut enc = RawEncoder;
        let (frame, stride) = make_test_frame(100, 100);
        let mut no_swap = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 1, 1, false, &mut no_swap)
            .unwrap();
        let mut swapped = Vec::new();
        enc.encode_rect_into(&frame, stride, 0, 0, 1, 1, true, &mut swapped)
            .unwrap();
        assert_eq!(no_swap, vec![0, 0, 255, 255]);
        assert_eq!(swapped, vec![255, 0, 0, 255]);
    }

    #[test]
    fn test_raw_encode_subregion() {
        let mut enc = RawEncoder;
        let stride = 100 * 4;
        let frame = vec![42u8; stride * 100];
        let mut out = Vec::new();
        enc.encode_rect_into(&frame, stride, 10, 10, 5, 5, false, &mut out)
            .unwrap();
        assert_eq!(out.len(), 5 * 5 * 4);
        assert!(out.iter().all(|&b| b == 42));
    }
}
