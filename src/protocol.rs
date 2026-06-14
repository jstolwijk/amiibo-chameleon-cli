use std::io::{Read, Write};

use crate::error::{Error, Result};

const SOF: u8 = 0x11;
const SOF_LRC: u8 = 0xEF;
const HEADER_SIZE: usize = 9;
const MAX_PAYLOAD_SIZE: usize = 512;
pub const STATUS_SUCCESS: u16 = 0x68;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub command: u16,
    pub status: u16,
    pub payload: Vec<u8>,
}

pub trait Transport: Read + Write {}
impl<T: Read + Write + ?Sized> Transport for T {}

pub fn encode(command: u16, payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() > MAX_PAYLOAD_SIZE {
        return Err(Error::Protocol(format!(
            "payload exceeds {MAX_PAYLOAD_SIZE} bytes"
        )));
    }

    let length = payload.len() as u16;
    let mut frame = Vec::with_capacity(payload.len() + 10);
    frame.extend_from_slice(&[SOF, SOF_LRC]);
    frame.extend_from_slice(&command.to_be_bytes());
    frame.extend_from_slice(&0_u16.to_be_bytes());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.push(lrc(&frame));
    frame.extend_from_slice(payload);
    frame.push(lrc(&frame));
    Ok(frame)
}

pub fn read_response<T: Transport + ?Sized>(
    transport: &mut T,
    expected_command: u16,
) -> Result<Response> {
    let mut header = [0_u8; HEADER_SIZE];
    transport.read_exact(&mut header)?;

    if header[0] != SOF || header[1] != SOF_LRC {
        return Err(Error::Protocol("invalid frame start".into()));
    }
    if header[8] != lrc(&header[..8]) {
        return Err(Error::Protocol("invalid header checksum".into()));
    }

    let command = u16::from_be_bytes([header[2], header[3]]);
    if command != expected_command {
        return Err(Error::Protocol(format!(
            "expected response command {expected_command}, received {command}"
        )));
    }
    let status = u16::from_be_bytes([header[4], header[5]]);
    let length = u16::from_be_bytes([header[6], header[7]]) as usize;
    if length > MAX_PAYLOAD_SIZE {
        return Err(Error::Protocol(format!(
            "response payload exceeds {MAX_PAYLOAD_SIZE} bytes"
        )));
    }

    let mut payload_and_lrc = vec![0_u8; length + 1];
    transport.read_exact(&mut payload_and_lrc)?;
    let received_lrc = payload_and_lrc
        .pop()
        .expect("checksum byte is always present");

    let mut checksum_data = header.to_vec();
    checksum_data.extend_from_slice(&payload_and_lrc);
    if received_lrc != lrc(&checksum_data) {
        return Err(Error::Protocol("invalid payload checksum".into()));
    }

    Ok(Response {
        command,
        status,
        payload: payload_and_lrc,
    })
}

pub fn transact<T: Transport + ?Sized>(
    transport: &mut T,
    command: u16,
    payload: &[u8],
) -> Result<Response> {
    let response = transact_raw(transport, command, payload)?;
    if response.status != STATUS_SUCCESS {
        return Err(Error::Protocol(format!(
            "command {command} failed with status 0x{:04X}",
            response.status
        )));
    }
    Ok(response)
}

pub fn transact_raw<T: Transport + ?Sized>(
    transport: &mut T,
    command: u16,
    payload: &[u8],
) -> Result<Response> {
    let request = encode(command, payload)?;
    transport.write_all(&request)?;
    transport.flush()?;
    read_response(transport, command)
}

fn lrc(bytes: &[u8]) -> u8 {
    0_u8.wrapping_sub(bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{encode, read_response};

    #[test]
    fn encodes_empty_request_frame() {
        assert_eq!(
            encode(1000, &[]).unwrap(),
            [0x11, 0xEF, 0x03, 0xE8, 0, 0, 0, 0, 0x15, 0]
        );
    }

    #[test]
    fn decodes_valid_response() {
        let frame = response_frame(1000, 0x68, &[2, 1]);
        let response = read_response(&mut Cursor::new(frame), 1000).unwrap();
        assert_eq!(response.status, 0x68);
        assert_eq!(response.payload, [2, 1]);
    }

    #[test]
    fn rejects_bad_header_checksum() {
        let mut frame = response_frame(1000, 0x68, &[2, 1]);
        frame[8] ^= 1;
        let error = read_response(&mut Cursor::new(frame), 1000).unwrap_err();
        assert!(error.to_string().contains("header checksum"));
    }

    #[test]
    fn rejects_bad_payload_checksum() {
        let mut frame = response_frame(1000, 0x68, &[2, 1]);
        *frame.last_mut().unwrap() ^= 1;
        let error = read_response(&mut Cursor::new(frame), 1000).unwrap_err();
        assert!(error.to_string().contains("payload checksum"));
    }

    fn response_frame(command: u16, status: u16, payload: &[u8]) -> Vec<u8> {
        let mut frame = encode(command, payload).unwrap();
        frame[4..6].copy_from_slice(&status.to_be_bytes());
        frame[8] = super::lrc(&frame[..8]);
        let last = frame.len() - 1;
        frame[last] = super::lrc(&frame[..last]);
        frame
    }
}
