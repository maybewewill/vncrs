#[derive(Debug, Clone, Copy)]
pub struct PixelFormat {
    pub bits_per_pixel: u8,
    pub depth: u8,
    pub big_endian: bool,
    pub true_colour: bool,
    pub red_max: u16,
    pub green_max: u16,
    pub blue_max: u16,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
}

impl PixelFormat {
    pub fn bgra32() -> Self {
        Self {
            bits_per_pixel: 32,
            depth: 24,
            big_endian: false,
            true_colour: true,
            red_max: 255,
            green_max: 255,
            blue_max: 255,
            red_shift: 16,  // R
            green_shift: 8, // G
            blue_shift: 0,  // B
        }
    }

    pub fn needs_bgr_swap(&self) -> bool {
        self.red_shift < self.blue_shift
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0] = self.bits_per_pixel;
        buf[1] = self.depth;
        buf[2] = self.big_endian as u8;
        buf[3] = self.true_colour as u8;
        buf[4..6].copy_from_slice(&self.red_max.to_be_bytes());
        buf[6..8].copy_from_slice(&self.green_max.to_be_bytes());
        buf[8..10].copy_from_slice(&self.blue_max.to_be_bytes());
        buf[10] = self.red_shift;
        buf[11] = self.green_shift;
        buf[12] = self.blue_shift;
        buf
    }

    pub fn from_bytes(buf: &[u8; 16]) -> Self {
        Self {
            bits_per_pixel: buf[0],
            depth: buf[1],
            big_endian: buf[2] != 0,
            true_colour: buf[3] != 0,
            red_max: u16::from_be_bytes([buf[4], buf[5]]),
            green_max: u16::from_be_bytes([buf[6], buf[7]]),
            blue_max: u16::from_be_bytes([buf[8], buf[9]]),
            red_shift: buf[10],
            green_shift: buf[11],
            blue_shift: buf[12],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bgra32_roundtrip() {
        let pf = PixelFormat::bgra32();
        let bytes = pf.to_bytes();
        let pf2 = PixelFormat::from_bytes(&bytes);

        assert_eq!(pf.bits_per_pixel, pf2.bits_per_pixel);
        assert_eq!(pf.red_shift, pf2.red_shift);
        assert_eq!(pf.green_shift, pf2.green_shift);
        assert_eq!(pf.blue_shift, pf2.blue_shift);
        assert_eq!(pf.red_max, pf2.red_max);
    }

    #[test]
    fn test_bgra32_values() {
        let pf = PixelFormat::bgra32();
        assert_eq!(pf.bits_per_pixel, 32);
        assert_eq!(pf.depth, 24);
        assert_eq!(pf.red_shift, 16);
        assert_eq!(pf.green_shift, 8);
        assert_eq!(pf.blue_shift, 0);
        assert!(!pf.big_endian);
        assert!(pf.true_colour);
    }

    #[test]
    fn test_needs_bgr_swap() {
        let bgra = PixelFormat::bgra32();
        assert!(!bgra.needs_bgr_swap());

        let mut rgba = PixelFormat::bgra32();
        rgba.red_shift = 0;
        rgba.blue_shift = 16;
        assert!(rgba.needs_bgr_swap()); // red_shift=0 < blue_shift=16
    }

    #[test]
    fn test_to_bytes_length() {
        let pf = PixelFormat::bgra32();
        let bytes = pf.to_bytes();
        assert_eq!(bytes.len(), 16);
    }
}
