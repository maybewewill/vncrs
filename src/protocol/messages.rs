use crate::error::{Result, VncError};
use std::io::Read;

#[derive(Debug)]
pub enum ClientMessage {
    SetPixelFormat {
        format: [u8; 16],
    },
    SetEncodings {
        encodings: Vec<i32>,
    },
    FramebufferUpdateRequest {
        incremental: bool,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    },
    KeyEvent {
        down: bool,
        key: u32,
    },
    PointerEvent {
        buttons: u8,
        x: u16,
        y: u16,
    },
    ClientCutText {
        text: String,
    },
}

impl ClientMessage {
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut msg_type = [0u8; 1];
        reader.read_exact(&mut msg_type)?;

        match msg_type[0] {
            0 => {
                let mut buf = [0u8; 19]; // 3 padding + 16 pixel format
                reader.read_exact(&mut buf)?;
                let mut format = [0u8; 16];
                format.copy_from_slice(&buf[3..19]);
                Ok(ClientMessage::SetPixelFormat { format })
            }
            2 => {
                let mut buf = [0u8; 3];
                reader.read_exact(&mut buf)?;
                let num = u16::from_be_bytes([buf[1], buf[2]]) as usize;
                let mut encodings = Vec::with_capacity(num);
                for _ in 0..num {
                    let mut enc = [0u8; 4];
                    reader.read_exact(&mut enc)?;
                    encodings.push(i32::from_be_bytes(enc));
                }
                Ok(ClientMessage::SetEncodings { encodings })
            }
            3 => {
                let mut buf = [0u8; 9];
                reader.read_exact(&mut buf)?;
                Ok(ClientMessage::FramebufferUpdateRequest {
                    incremental: buf[0] != 0,
                    x: u16::from_be_bytes([buf[1], buf[2]]),
                    y: u16::from_be_bytes([buf[3], buf[4]]),
                    width: u16::from_be_bytes([buf[5], buf[6]]),
                    height: u16::from_be_bytes([buf[7], buf[8]]),
                })
            }
            4 => {
                let mut buf = [0u8; 7];
                reader.read_exact(&mut buf)?;
                Ok(ClientMessage::KeyEvent {
                    down: buf[0] != 0,
                    key: u32::from_be_bytes([buf[3], buf[4], buf[5], buf[6]]),
                })
            }
            5 => {
                let mut buf = [0u8; 5];
                reader.read_exact(&mut buf)?;
                Ok(ClientMessage::PointerEvent {
                    buttons: buf[0],
                    x: u16::from_be_bytes([buf[1], buf[2]]),
                    y: u16::from_be_bytes([buf[3], buf[4]]),
                })
            }
            6 => {
                let mut buf = [0u8; 7];
                reader.read_exact(&mut buf)?;
                let len = u32::from_be_bytes([buf[3], buf[4], buf[5], buf[6]]) as usize;
                let mut text_buf = vec![0u8; len];
                reader.read_exact(&mut text_buf)?;
                let text = String::from_utf8_lossy(&text_buf).to_string();
                Ok(ClientMessage::ClientCutText { text })
            }
            other => Err(VncError::Handshake(format!(
                "Unknown message type: {}",
                other
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_key_event() {
        let data: Vec<u8> = vec![
            4, // message type
            1, // down-flag
            0, 0, // padding
            0x00, 0x00, 0xFF, 0x0D, // key (Return)
        ];
        let mut cursor = Cursor::new(&data[1..]);

        let mut buf = [0u8; 7];
        cursor.read_exact(&mut buf).unwrap();
        let down = buf[0] != 0;
        let key = u32::from_be_bytes([buf[3], buf[4], buf[5], buf[6]]);

        assert!(down);
        assert_eq!(key, 0xFF0D);
    }

    #[test]
    fn test_parse_pointer_event() {
        let data: Vec<u8> = vec![
            5, // message type
            1, // buttons
            0, 100, // x
            0, 200, // y
        ];
        let mut cursor = Cursor::new(data);
        let msg = ClientMessage::read_from(&mut cursor).unwrap();

        match msg {
            ClientMessage::PointerEvent { buttons, x, y } => {
                assert_eq!(buttons, 1);
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            _ => panic!("Expected PointerEvent"),
        }
    }

    #[test]
    fn test_parse_framebuffer_update_request() {
        let data: Vec<u8> = vec![
            3, // message type
            1, // incremental
            0, 0, // x
            0, 0, // y
            0x07, 0x80, // width = 1920
            0x04, 0x38, // height = 1080
        ];
        let mut cursor = Cursor::new(data);
        let msg = ClientMessage::read_from(&mut cursor).unwrap();

        match msg {
            ClientMessage::FramebufferUpdateRequest {
                incremental,
                x,
                y,
                width,
                height,
            } => {
                assert!(incremental);
                assert_eq!(x, 0);
                assert_eq!(y, 0);
                assert_eq!(width, 1920);
                assert_eq!(height, 1080);
            }
            _ => panic!("Expected FramebufferUpdateRequest"),
        }
    }

    #[test]
    fn test_parse_set_encodings() {
        let data: Vec<u8> = vec![
            2, // message type
            0, // padding
            0, 3, // number of encodings = 3
            0, 0, 0, 5, // Hextile
            0, 0, 0, 6, // Zlib
            0, 0, 0, 0, // Raw
        ];
        let mut cursor = Cursor::new(data);
        let msg = ClientMessage::read_from(&mut cursor).unwrap();

        match msg {
            ClientMessage::SetEncodings { encodings } => {
                assert_eq!(encodings, vec![5, 6, 0]);
            }
            _ => panic!("Expected SetEncodings"),
        }
    }

    #[test]
    fn test_parse_client_cut_text() {
        let text = b"Hello VNC!";
        let mut data: Vec<u8> = vec![
            6, // message type
            0, 0, 0, // padding
        ];
        data.extend_from_slice(&(text.len() as u32).to_be_bytes());
        data.extend_from_slice(text);

        let mut cursor = Cursor::new(data);
        let msg = ClientMessage::read_from(&mut cursor).unwrap();

        match msg {
            ClientMessage::ClientCutText { text: t } => {
                assert_eq!(t, "Hello VNC!");
            }
            _ => panic!("Expected ClientCutText"),
        }
    }
}
