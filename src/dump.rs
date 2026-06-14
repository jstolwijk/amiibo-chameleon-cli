use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub const DUMP_SIZE: usize = 540;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedDump {
    pub path: PathBuf,
    pub bin_root: PathBuf,
    pub uid: [u8; 7],
    pub manufacturer_warning: bool,
}

impl ValidatedDump {
    pub fn read(&self) -> Result<Vec<u8>> {
        let current = validate(&self.path, &self.bin_root)?;
        if current.uid != self.uid {
            return Err(Error::InvalidDump {
                path: self.path.clone(),
                reason: "UID changed after the database was indexed".into(),
            });
        }
        fs::read(&current.path).map_err(Into::into)
    }
}

pub fn validate(path: &Path, bin_root: &Path) -> Result<ValidatedDump> {
    let canonical_root = bin_root.canonicalize()?;
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(Error::InvalidDump {
            path: canonical_path,
            reason: "path escapes the Amiibo Bin directory".into(),
        });
    }

    let bytes = fs::read(&canonical_path)?;
    validate_bytes(&canonical_path, &canonical_root, &bytes)
}

fn validate_bytes(path: &Path, bin_root: &Path, bytes: &[u8]) -> Result<ValidatedDump> {
    if bytes.len() != DUMP_SIZE {
        return Err(Error::InvalidDump {
            path: path.to_owned(),
            reason: format!("expected {DUMP_SIZE} bytes, found {}", bytes.len()),
        });
    }

    let expected_bcc0 = 0x88 ^ bytes[0] ^ bytes[1] ^ bytes[2];
    if bytes[3] != expected_bcc0 {
        return Err(Error::InvalidDump {
            path: path.to_owned(),
            reason: format!(
                "invalid UID check byte BCC0: expected {expected_bcc0:02X}, found {:02X}",
                bytes[3]
            ),
        });
    }

    let expected_bcc1 = bytes[4] ^ bytes[5] ^ bytes[6] ^ bytes[7];
    if bytes[8] != expected_bcc1 {
        return Err(Error::InvalidDump {
            path: path.to_owned(),
            reason: format!(
                "invalid UID check byte BCC1: expected {expected_bcc1:02X}, found {:02X}",
                bytes[8]
            ),
        });
    }

    if bytes[12] != 0xE1 || bytes[13] >> 4 != 1 || bytes[14] != 0x3E {
        return Err(Error::InvalidDump {
            path: path.to_owned(),
            reason: "invalid NTAG215 capability container on page 3".into(),
        });
    }
    if bytes[520..524] != [0x01, 0x00, 0x0F, 0xBD] {
        return Err(Error::InvalidDump {
            path: path.to_owned(),
            reason: "unexpected NTAG215 dynamic lock page at page 130".into(),
        });
    }
    if bytes[524..528] != [0x00, 0x00, 0x00, 0x04] || bytes[528..532] != [0x5F, 0x00, 0x00, 0x00] {
        return Err(Error::InvalidDump {
            path: path.to_owned(),
            reason: "unexpected NTAG215 configuration pages 131-132".into(),
        });
    }
    if bytes[532..540] != [0; 8] {
        return Err(Error::InvalidDump {
            path: path.to_owned(),
            reason: "PWD/PACK pages must contain unreadable zero placeholders".into(),
        });
    }

    Ok(ValidatedDump {
        path: path.to_owned(),
        bin_root: bin_root.to_owned(),
        uid: [
            bytes[0], bytes[1], bytes[2], bytes[4], bytes[5], bytes[6], bytes[7],
        ],
        manufacturer_warning: bytes[0] != 0x04,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{DUMP_SIZE, validate_bytes};

    fn valid_dump() -> Vec<u8> {
        let mut bytes = vec![0; DUMP_SIZE];
        bytes[0..3].copy_from_slice(&[0x04, 0x12, 0x34]);
        bytes[3] = 0x88 ^ bytes[0] ^ bytes[1] ^ bytes[2];
        bytes[4..8].copy_from_slice(&[0x56, 0x78, 0x9A, 0xBC]);
        bytes[8] = bytes[4] ^ bytes[5] ^ bytes[6] ^ bytes[7];
        bytes[12..16].copy_from_slice(&[0xE1, 0x10, 0x3E, 0]);
        bytes[520..524].copy_from_slice(&[0x01, 0x00, 0x0F, 0xBD]);
        bytes[524..528].copy_from_slice(&[0x00, 0x00, 0x00, 0x04]);
        bytes[528..532].copy_from_slice(&[0x5F, 0, 0, 0]);
        bytes
    }

    #[test]
    fn extracts_uid_from_valid_dump() {
        let dump = validate_bytes(Path::new("Mario.bin"), Path::new("."), &valid_dump()).unwrap();
        assert_eq!(dump.uid, [0x04, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]);
        assert!(!dump.manufacturer_warning);
    }

    #[test]
    fn rejects_wrong_size() {
        let error = validate_bytes(Path::new("short.bin"), Path::new("."), &[0; 10]).unwrap_err();
        assert!(error.to_string().contains("expected 540 bytes"));
    }

    #[test]
    fn rejects_invalid_bcc() {
        let mut bytes = valid_dump();
        bytes[3] ^= 1;
        let error = validate_bytes(Path::new("bad.bin"), Path::new("."), &bytes).unwrap_err();
        assert!(error.to_string().contains("BCC0"));
    }

    #[test]
    fn rejects_invalid_ntag215_configuration() {
        let mut bytes = valid_dump();
        bytes[14] = 0;
        let error = validate_bytes(Path::new("bad.bin"), Path::new("."), &bytes).unwrap_err();
        assert!(error.to_string().contains("capability container"));
    }

    #[test]
    fn rejects_non_placeholder_password_pages() {
        let mut bytes = valid_dump();
        bytes[532] = 1;
        let error = validate_bytes(Path::new("bad.bin"), Path::new("."), &bytes).unwrap_err();
        assert!(error.to_string().contains("PWD/PACK"));
    }
}
