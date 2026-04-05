use crate::error::{Result, VncError};
use crate::protocol::auth;
use crate::protocol::pixel_format::PixelFormat;
use std::io::{Read, Write};

pub fn perform_handshake<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    width: u16,
    height: u16,
    name: &str,
    pixel_format: &PixelFormat,
    password: Option<&str>,
) -> Result<()> {
    writer.write_all(b"RFB 003.008\n")?;
    writer.flush()?;

    let mut version = [0u8; 12];
    reader.read_exact(&mut version)?;
    let version_str = String::from_utf8_lossy(&version);
    println!("📡 Client version: {}", version_str.trim());

    if let Some(pass) = password {
        writer.write_all(&[1u8, 2u8])?;
        writer.flush()?;

        let mut chosen = [0u8; 1];
        reader.read_exact(&mut chosen)?;

        if chosen[0] != 2 {
            return Err(VncError::Handshake(format!(
                "Client chose unsupported security type: {}",
                chosen[0]
            )));
        }

        auth::perform_vnc_auth(reader, writer, pass)?;
    } else {
        writer.write_all(&[1u8, 1u8])?;
        writer.flush()?;

        let mut chosen = [0u8; 1];
        reader.read_exact(&mut chosen)?;

        writer.write_all(&0u32.to_be_bytes())?;
        writer.flush()?;
    }

    let mut shared_flag = [0u8; 1];
    reader.read_exact(&mut shared_flag)?;

    let mut init: Vec<u8> = Vec::new();
    init.extend_from_slice(&width.to_be_bytes());
    init.extend_from_slice(&height.to_be_bytes());
    init.extend_from_slice(&pixel_format.to_bytes());

    let name_bytes = name.as_bytes();
    init.extend_from_slice(&(name_bytes.len() as u32).to_be_bytes());
    init.extend_from_slice(name_bytes);

    writer.write_all(&init)?;
    writer.flush()?;

    Ok(())
}
