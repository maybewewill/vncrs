pub fn keysym_to_unicode(keysym: u32) -> Option<char> {
    match keysym {
        k if k >= 0x0100_0000 => char::from_u32(k - 0x0100_0000),

        0x0020..=0x007E => Some(keysym as u8 as char),
        0x00A0..=0x00FF => char::from_u32(keysym),

        0x06A3 => Some('ё'),
        0x06B3 => Some('Ё'),

        0x06C0 => Some('ю'),
        0x06C1 => Some('а'),
        0x06C2 => Some('б'),
        0x06C3 => Some('ц'),
        0x06C4 => Some('д'),
        0x06C5 => Some('е'),
        0x06C6 => Some('ф'),
        0x06C7 => Some('г'),
        0x06C8 => Some('х'),
        0x06C9 => Some('и'),
        0x06CA => Some('й'),
        0x06CB => Some('к'),
        0x06CC => Some('л'),
        0x06CD => Some('м'),
        0x06CE => Some('н'),
        0x06CF => Some('о'),
        0x06D0 => Some('п'),
        0x06D1 => Some('я'),
        0x06D2 => Some('р'),
        0x06D3 => Some('с'),
        0x06D4 => Some('т'),
        0x06D5 => Some('у'),
        0x06D6 => Some('ж'),
        0x06D7 => Some('в'),
        0x06D8 => Some('ь'),
        0x06D9 => Some('ы'),
        0x06DA => Some('з'),
        0x06DB => Some('ш'),
        0x06DC => Some('э'),
        0x06DD => Some('щ'),
        0x06DE => Some('ч'),
        0x06DF => Some('ъ'),

        0x06E0 => Some('Ю'),
        0x06E1 => Some('А'),
        0x06E2 => Some('Б'),
        0x06E3 => Some('Ц'),
        0x06E4 => Some('Д'),
        0x06E5 => Some('Е'),
        0x06E6 => Some('Ф'),
        0x06E7 => Some('Г'),
        0x06E8 => Some('Х'),
        0x06E9 => Some('И'),
        0x06EA => Some('Й'),
        0x06EB => Some('К'),
        0x06EC => Some('Л'),
        0x06ED => Some('М'),
        0x06EE => Some('Н'),
        0x06EF => Some('О'),
        0x06F0 => Some('П'),
        0x06F1 => Some('Я'),
        0x06F2 => Some('Р'),
        0x06F3 => Some('С'),
        0x06F4 => Some('Т'),
        0x06F5 => Some('У'),
        0x06F6 => Some('Ж'),
        0x06F7 => Some('В'),
        0x06F8 => Some('Ь'),
        0x06F9 => Some('Ы'),
        0x06FA => Some('З'),
        0x06FB => Some('Ш'),
        0x06FC => Some('Э'),
        0x06FD => Some('Щ'),
        0x06FE => Some('Ч'),
        0x06FF => Some('Ъ'),

        _ => None,
    }
}

pub fn is_control_key(keysym: u32) -> bool {
    matches!(
        keysym,
        0xFF08..=0xFF1B
        | 0xFF50..=0xFF58
        | 0xFF63
        | 0xFF7F
        | 0xFFBE..=0xFFC9
        | 0xFFE1..=0xFFEE
        | 0xFFFF
        | 0xFF61
        | 0xFF67
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii() {
        assert_eq!(keysym_to_unicode(0x0041), Some('A'));
        assert_eq!(keysym_to_unicode(0x0061), Some('a'));
        assert_eq!(keysym_to_unicode(0x0020), Some(' '));
    }

    #[test]
    fn test_cyrillic() {
        assert_eq!(keysym_to_unicode(0x06C1), Some('а'));
        assert_eq!(keysym_to_unicode(0x06E1), Some('А'));
        assert_eq!(keysym_to_unicode(0x06A3), Some('ё'));
        assert_eq!(keysym_to_unicode(0x06B3), Some('Ё'));
    }

    #[test]
    fn test_unicode_keysym() {
        assert_eq!(keysym_to_unicode(0x01000410), Some('А'));
        assert_eq!(keysym_to_unicode(0x010020AC), Some('€'));
    }

    #[test]
    fn test_control_keys() {
        assert!(is_control_key(0xFF08));
        assert!(is_control_key(0xFFE1));
        assert!(!is_control_key(0x0041));
    }
}
