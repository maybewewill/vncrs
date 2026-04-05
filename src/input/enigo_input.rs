use super::keysym;
use super::{InputHandler, ScrollDirection};
use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};

pub struct EnigoInput {
    enigo: Enigo,
    ctrl_alt_held: u8,
    shift_held: u8,
}

impl EnigoInput {
    pub fn new() -> Self {
        Self {
            enigo: Enigo::new(&Settings::default()).unwrap(),
            ctrl_alt_held: 0,
            shift_held: 0,
        }
    }

    fn map_control_key(ks: u32) -> Option<Key> {
        match ks {
            0xFF08 => Some(Key::Backspace),
            0xFF09 => Some(Key::Tab),
            0xFF0D | 0xFF8D => Some(Key::Return),
            0xFF1B => Some(Key::Escape),
            0xFFFF => Some(Key::Delete),
            0xFF50 => Some(Key::Home),
            0xFF51 => Some(Key::LeftArrow),
            0xFF52 => Some(Key::UpArrow),
            0xFF53 => Some(Key::RightArrow),
            0xFF54 => Some(Key::DownArrow),
            0xFF55 => Some(Key::PageUp),
            0xFF56 => Some(Key::PageDown),
            0xFF57 => Some(Key::End),
            0xFFBE => Some(Key::F1),
            0xFFBF => Some(Key::F2),
            0xFFC0 => Some(Key::F3),
            0xFFC1 => Some(Key::F4),
            0xFFC2 => Some(Key::F5),
            0xFFC3 => Some(Key::F6),
            0xFFC4 => Some(Key::F7),
            0xFFC5 => Some(Key::F8),
            0xFFC6 => Some(Key::F9),
            0xFFC7 => Some(Key::F10),
            0xFFC8 => Some(Key::F11),
            0xFFC9 => Some(Key::F12),
            0xFFE5 => Some(Key::CapsLock),
            0xFF63 => Some(Key::Other(0x2D)), // Insert
            _ => None,
        }
    }

    fn map_modifier(ks: u32) -> Option<Key> {
        match ks {
            0xFFE1 => Some(Key::LShift),
            0xFFE2 => Some(Key::RShift),
            0xFFE3 => Some(Key::LControl),
            0xFFE4 => Some(Key::RControl),
            0xFFE9 => Some(Key::Alt),
            0xFFEA => Some(Key::Alt),
            0xFFEB | 0xFFEC => Some(Key::Meta),
            _ => None,
        }
    }

    fn is_modifier(ks: u32) -> bool {
        matches!(ks, 0xFFE1..=0xFFEE)
    }

    fn is_ctrl_or_alt(ks: u32) -> bool {
        matches!(ks, 0xFFE3 | 0xFFE4 | 0xFFE9 | 0xFFEA)
    }

    fn is_shift(ks: u32) -> bool {
        matches!(ks, 0xFFE1 | 0xFFE2)
    }

    fn char_to_vk(ch: char) -> Option<u16> {
        match ch {
            // Digits and their Shift variants
            '0' | ')' => Some(0x30),
            '1' | '!' => Some(0x31),
            '2' | '@' => Some(0x32),
            '3' | '#' => Some(0x33),
            '4' | '$' => Some(0x34),
            '5' | '%' => Some(0x35),
            '6' | '^' => Some(0x36),
            '7' | '&' => Some(0x37),
            '8' | '*' => Some(0x38),
            '9' | '(' => Some(0x39),
            // Letters (VK_A = 0x41 … VK_Z = 0x5A)
            'a'..='z' => Some(ch as u16 - b'a' as u16 + 0x41),
            'A'..='Z' => Some(ch as u16 - b'A' as u16 + 0x41),
            // OEM symbol keys (US keyboard layout)
            ';' | ':' => Some(0xBA),  // VK_OEM_1
            '=' | '+' => Some(0xBB),  // VK_OEM_PLUS
            ',' | '<' => Some(0xBC),  // VK_OEM_COMMA
            '-' | '_' => Some(0xBD),  // VK_OEM_MINUS
            '.' | '>' => Some(0xBE),  // VK_OEM_PERIOD
            '/' | '?' => Some(0xBF),  // VK_OEM_2
            '`' | '~' => Some(0xC0),  // VK_OEM_3
            '[' | '{' => Some(0xDB),  // VK_OEM_4
            '\\' | '|' => Some(0xDC), // VK_OEM_5
            ']' | '}' => Some(0xDD),  // VK_OEM_6
            '\'' | '"' => Some(0xDE), // VK_OEM_7
            _ => None,
        }
    }
}

impl InputHandler for EnigoInput {
    fn move_mouse(&mut self, x: u16, y: u16) {
        let _ = self.enigo.move_mouse(x as i32, y as i32, Coordinate::Abs);
    }

    fn mouse_button(&mut self, button: u8, pressed: bool) {
        let dir = if pressed {
            Direction::Press
        } else {
            Direction::Release
        };
        let btn = match button {
            1 => Button::Left,
            2 => Button::Middle,
            3 => Button::Right,
            _ => return,
        };
        let _ = self.enigo.button(btn, dir);
    }

    fn scroll(&mut self, direction: ScrollDirection) {
        let (length, axis) = match direction {
            ScrollDirection::Up => (3, Axis::Vertical),
            ScrollDirection::Down => (-3, Axis::Vertical),
            ScrollDirection::Left => (-3, Axis::Horizontal),
            ScrollDirection::Right => (3, Axis::Horizontal),
        };
        let _ = self.enigo.scroll(length, axis);
    }

    fn key_event(&mut self, ks: u32, down: bool) {
        let dir = if down {
            Direction::Press
        } else {
            Direction::Release
        };

        if Self::is_modifier(ks) {
            if Self::is_ctrl_or_alt(ks) {
                if down {
                    self.ctrl_alt_held += 1;
                } else if self.ctrl_alt_held > 0 {
                    self.ctrl_alt_held -= 1;
                }
            }
            if Self::is_shift(ks) {
                if down {
                    self.shift_held += 1;
                } else if self.shift_held > 0 {
                    self.shift_held -= 1;
                }
            }
            if let Some(key) = Self::map_modifier(ks) {
                let _ = self.enigo.key(key, dir);
            }
            return;
        }

        if let Some(key) = Self::map_control_key(ks) {
            let _ = self.enigo.key(key, dir);
            return;
        }

        if let Some(ch) = keysym::keysym_to_unicode(ks) {
            let has_modifiers = self.ctrl_alt_held > 0 || self.shift_held > 0;

            if let Some(vk) = Self::char_to_vk(ch) {
                if has_modifiers {
                    let _ = self.enigo.key(Key::Other(vk.into()), dir);
                } else if down {
                    let _ = self.enigo.text(&ch.to_string());
                }
            } else if self.ctrl_alt_held > 0 {
                let _ = self.enigo.key(Key::Unicode(ch), dir);
            } else if down {
                let _ = self.enigo.text(&ch.to_string());
            }
        }
    }
}
