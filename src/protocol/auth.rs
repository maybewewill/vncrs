use crate::error::{Result, VncError};
use cipher::{BlockEncrypt, KeyInit};
use des::Des;
use rand::RngCore;
use std::io::{Read, Write};

fn reverse_bits(byte: u8) -> u8 {
    let mut result = 0u8;
    for i in 0..8 {
        result |= ((byte >> i) & 1) << (7 - i);
    }
    result
}

fn password_to_key(password: &str) -> [u8; 8] {
    let mut key = [0u8; 8];
    for (i, &b) in password.as_bytes().iter().take(8).enumerate() {
        key[i] = reverse_bits(b);
    }
    key
}

fn encrypt_challenge(key: &[u8; 8], challenge: &[u8; 16]) -> [u8; 16] {
    let cipher = Des::new_from_slice(key).unwrap();
    let mut result = [0u8; 16];
    result.copy_from_slice(challenge);

    let (block1, block2) = result.split_at_mut(8);
    cipher.encrypt_block(block1.into());
    cipher.encrypt_block(block2.into());

    result
}

pub fn perform_vnc_auth<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    password: &str,
) -> Result<()> {
    let mut challenge = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut challenge);

    writer.write_all(&challenge)?;
    writer.flush()?;

    let mut client_response = [0u8; 16];
    reader.read_exact(&mut client_response)?;

    let key = password_to_key(password);
    let expected = encrypt_challenge(&key, &challenge);

    if client_response == expected {
        writer.write_all(&0u32.to_be_bytes())?;
        writer.flush()?;
        println!("🔓 Authentication successful");
        Ok(())
    } else {
        writer.write_all(&1u32.to_be_bytes())?;
        let reason = b"Authentication failed";
        writer.write_all(&(reason.len() as u32).to_be_bytes())?;
        writer.write_all(reason)?;
        writer.flush()?;
        println!("🔒 Authentication FAILED");
        Err(VncError::Handshake("Authentication failed".into()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse_bits() {
        assert_eq!(reverse_bits(0b10000000), 0b00000001);
        assert_eq!(reverse_bits(0b11001010), 0b01010011);
        assert_eq!(reverse_bits(0xFF), 0xFF);
        assert_eq!(reverse_bits(0x00), 0x00);
    }

    #[test]
    fn test_password_to_key_length() {
        let key = password_to_key("test");
        assert_eq!(key.len(), 8);
    }

    #[test]
    fn test_password_to_key_truncation() {
        let key_short = password_to_key("ab");
        let key_long = password_to_key("abcdefghijklmnop");
        assert_eq!(key_short[0], key_long[0]);
        assert_eq!(key_short[1], key_long[1]);
        assert_eq!(key_short[2], 0);
    }

    #[test]
    fn test_encrypt_challenge_deterministic() {
        let key = password_to_key("secret");
        let challenge = [1u8; 16];
        let result1 = encrypt_challenge(&key, &challenge);
        let result2 = encrypt_challenge(&key, &challenge);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_auth_success() {
        let password = "secret";
        let key = password_to_key(password);
        let challenge = [42u8; 16];

        let client_response = encrypt_challenge(&key, &challenge);
        let expected = encrypt_challenge(&key, &challenge);
        assert_eq!(client_response, expected);
    }

    #[test]
    fn test_auth_wrong_password() {
        let server_key = password_to_key("correct");
        let client_key = password_to_key("wrong");
        let challenge = [7u8; 16];

        let server_expected = encrypt_challenge(&server_key, &challenge);
        let client_response = encrypt_challenge(&client_key, &challenge);
        assert_ne!(server_expected, client_response);
    }
}
