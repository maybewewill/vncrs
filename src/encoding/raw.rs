use super::Encoder;
use crate::error::Result;

pub struct RawEncoder;

impl Encoder for RawEncoder {
    fn encoding_id(&self) -> i32 {
        0
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
        let mut data = Vec::with_capacity(row_bytes * h as usize);

        for row in y..y + h {
            let start = row as usize * stride + x as usize * 4;
            let end = start + row_bytes;
            data.extend_from_slice(&pixels[start..end]);
        }

        if swap_rb {
            for pixel in data.chunks_exact_mut(4) {
                pixel.swap(0, 2); // B ↔ R
            }
        }

        Ok(data)
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
                frame[off] = 0; // B
                frame[off + 1] = 0; // G
                frame[off + 2] = 255; // R
                frame[off + 3] = 255; // A
            }
        }
        (frame, stride)
    }

    #[test]
    fn test_raw_encode_size() {
        let mut enc = RawEncoder;
        let (frame, stride) = make_test_frame(100, 100);

        let data = enc
            .encode_rect(&frame, stride, 0, 0, 10, 10, false)
            .unwrap();
        assert_eq!(data.len(), 10 * 10 * 4);
    }

    #[test]
    fn test_raw_encode_with_swap() {
        let mut enc = RawEncoder;
        let (frame, stride) = make_test_frame(100, 100);

        let no_swap = enc.encode_rect(&frame, stride, 0, 0, 1, 1, false).unwrap();
        let swapped = enc.encode_rect(&frame, stride, 0, 0, 1, 1, true).unwrap();

        assert_eq!(no_swap, vec![0, 0, 255, 255]);

        assert_eq!(swapped, vec![255, 0, 0, 255]);
    }

    #[test]
    fn test_raw_encode_subregion() {
        let mut enc = RawEncoder;
        let stride = 100 * 4;
        let frame = vec![42u8; stride * 100];

        let data = enc
            .encode_rect(&frame, stride, 10, 10, 5, 5, false)
            .unwrap();
        assert_eq!(data.len(), 5 * 5 * 4);
        assert!(data.iter().all(|&b| b == 42));
    }
}
