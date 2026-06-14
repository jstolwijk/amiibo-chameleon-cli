use std::fs::{self, OpenOptions};
use std::io::BufWriter;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::device::{NtagBackup, TagType};
use crate::error::{Error, Result};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupArtifact {
    pub format_version: u8,
    pub firmware: String,
    pub git_version: String,
    pub slot: u8,
    pub hf_type: u16,
    pub hf_enabled: bool,
    pub hf_nickname: String,
    pub lf_type: u16,
    pub lf_enabled: bool,
    pub lf_nickname: String,
    pub uid: String,
    pub atqa: String,
    pub sak: String,
    pub ats: String,
    pub uid_magic: bool,
    pub write_mode: u8,
    pub detection_enabled: bool,
    pub counter: u32,
    pub counter_tearing: bool,
    pub version_data: String,
    pub signature_data: String,
    pub pages: String,
}

impl BackupArtifact {
    pub fn from_device(backup: NtagBackup) -> Self {
        Self {
            format_version: 1,
            firmware: format!("{}.{}", backup.firmware_major, backup.firmware_minor),
            git_version: backup.git_version,
            slot: backup.slot,
            hf_type: backup.hf_type.0,
            hf_enabled: backup.hf_enabled,
            hf_nickname: backup.hf_nickname,
            lf_type: backup.lf_type.0,
            lf_enabled: backup.lf_enabled,
            lf_nickname: backup.lf_nickname,
            uid: hex(&backup.uid),
            atqa: hex(&backup.atqa),
            sak: format!("{:02X}", backup.sak),
            ats: hex(&backup.ats),
            uid_magic: backup.uid_magic,
            write_mode: backup.write_mode,
            detection_enabled: backup.detection_enabled,
            counter: backup.counter,
            counter_tearing: backup.counter_tearing,
            version_data: hex(&backup.version_data),
            signature_data: hex(&backup.signature_data),
            pages: hex(&backup.pages),
        }
    }

    pub fn write_new(&self, path: &Path) -> Result<()> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        serde_json::to_writer_pretty(BufWriter::new(file), self)?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Self> {
        let artifact: Self = serde_json::from_slice(&fs::read(path)?)?;
        artifact.validate()?;
        Ok(artifact)
    }

    pub fn into_device(self, slot: u8) -> Result<NtagBackup> {
        self.validate()?;
        Ok(NtagBackup {
            firmware_major: 0,
            firmware_minor: 0,
            git_version: self.git_version,
            slot,
            hf_type: TagType(self.hf_type),
            hf_enabled: self.hf_enabled,
            hf_nickname: self.hf_nickname,
            lf_type: TagType(self.lf_type),
            lf_enabled: self.lf_enabled,
            lf_nickname: self.lf_nickname,
            uid: decode_hex("uid", &self.uid)?,
            atqa: decode_array("atqa", &self.atqa)?,
            sak: decode_array::<1>("sak", &self.sak)?[0],
            ats: decode_hex("ats", &self.ats)?,
            uid_magic: self.uid_magic,
            write_mode: self.write_mode,
            detection_enabled: self.detection_enabled,
            counter: self.counter,
            counter_tearing: self.counter_tearing,
            version_data: decode_hex("version_data", &self.version_data)?,
            signature_data: decode_hex("signature_data", &self.signature_data)?,
            pages: decode_hex("pages", &self.pages)?,
        })
    }

    fn validate(&self) -> Result<()> {
        if self.format_version != 1 {
            return Err(invalid(format!(
                "unsupported format_version {}",
                self.format_version
            )));
        }
        if !(1..=8).contains(&self.slot) {
            return Err(invalid(format!("slot {} is outside 1..=8", self.slot)));
        }
        if !(1100..=1108).contains(&self.hf_type) {
            return Err(invalid(format!(
                "HF tag type {} is not NTAG/MIFARE Ultralight",
                self.hf_type
            )));
        }
        if self.hf_nickname.len() > 32 || self.lf_nickname.len() > 32 {
            return Err(invalid("slot nickname exceeds 32 bytes"));
        }
        if self.write_mode > 4 {
            return Err(invalid(format!("invalid write mode {}", self.write_mode)));
        }
        if self.counter > 0xFF_FFFF {
            return Err(invalid("counter exceeds 24 bits"));
        }

        let uid = decode_hex("uid", &self.uid)?;
        if !matches!(uid.len(), 4 | 7 | 10) {
            return Err(invalid(format!(
                "UID is {} bytes, expected 4, 7, or 10",
                uid.len()
            )));
        }
        let _: [u8; 2] = decode_array("atqa", &self.atqa)?;
        let _: [u8; 1] = decode_array("sak", &self.sak)?;
        if decode_hex("ats", &self.ats)?.len() > u8::MAX as usize {
            return Err(invalid("ATS exceeds 255 bytes"));
        }
        if decode_hex("version_data", &self.version_data)?.len() != 8 {
            return Err(invalid("version_data must be 8 bytes"));
        }
        if decode_hex("signature_data", &self.signature_data)?.len() != 32 {
            return Err(invalid("signature_data must be 32 bytes"));
        }
        let expected_pages = match self.hf_type {
            1100 => 45,
            1101 => 135,
            1102 => 231,
            1103 => 16,
            1104 => 48,
            1105 | 1107 => 20,
            1106 => 41,
            1108 => 41,
            _ => unreachable!(),
        };
        let pages = decode_hex("pages", &self.pages)?;
        if pages.len() != expected_pages * 4 {
            return Err(invalid(format!(
                "pages contains {} bytes, expected {} for tag type {}",
                pages.len(),
                expected_pages * 4,
                self.hf_type
            )));
        }
        Ok(())
    }
}

pub fn write_recovery(backup: NtagBackup, device: &Path) -> Result<std::path::PathBuf> {
    let home = std::env::var_os("HOME").ok_or(Error::CacheUnavailable)?;
    let directory = std::path::PathBuf::from(home)
        .join("Library")
        .join("Caches")
        .join("amiibo")
        .join("recovery");
    fs::create_dir_all(&directory)?;
    let device_name = device
        .to_string_lossy()
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
                char::from(byte)
            } else {
                '_'
            }
        })
        .collect::<String>();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("system clock predates Unix epoch"))?
        .as_millis();
    let path = directory.join(format!(
        "{device_name}-slot-{}-{timestamp}.json",
        backup.slot
    ));
    BackupArtifact::from_device(backup).write_new(&path)?;
    Ok(path)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn decode_hex(field: &str, value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!("{field} is not even-length hexadecimal")));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("hexadecimal is ASCII");
            u8::from_str_radix(text, 16)
                .map_err(|_| invalid(format!("{field} contains invalid hexadecimal")))
        })
        .collect()
}

fn decode_array<const N: usize>(field: &str, value: &str) -> Result<[u8; N]> {
    decode_hex(field, value)?
        .try_into()
        .map_err(|bytes: Vec<u8>| {
            invalid(format!("{field} is {} bytes, expected {N}", bytes.len()))
        })
}

fn invalid(reason: impl Into<String>) -> Error {
    Error::InvalidBackup(reason.into())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::device::{NtagBackup, TagType};

    use super::BackupArtifact;

    fn artifact() -> BackupArtifact {
        BackupArtifact::from_device(NtagBackup {
            firmware_major: 2,
            firmware_minor: 1,
            git_version: "v2.1.0".into(),
            slot: 3,
            hf_type: TagType(1101),
            hf_enabled: true,
            hf_nickname: "Mario".into(),
            lf_type: TagType(100),
            lf_enabled: true,
            lf_nickname: "Badge".into(),
            uid: vec![0x04, 1, 2, 3, 4, 5, 6],
            atqa: [0x44, 0],
            sak: 0,
            ats: vec![],
            uid_magic: false,
            write_mode: 0,
            detection_enabled: false,
            counter: 0x123456,
            counter_tearing: false,
            version_data: vec![1; 8],
            signature_data: vec![2; 32],
            pages: vec![0; 540],
        })
    }

    #[test]
    fn serializes_binary_state_as_hex() {
        let artifact = artifact();
        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("\"uid\":\"04010203040506\""));
        assert!(json.contains("\"pages\":\"0000"));
    }

    #[test]
    fn reads_and_converts_valid_artifact() {
        let path = std::env::temp_dir().join(format!(
            "amiibo-backup-{}.json",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        artifact().write_new(&path).unwrap();
        let backup = BackupArtifact::read(&path).unwrap().into_device(7).unwrap();
        assert_eq!(backup.slot, 7);
        assert_eq!(backup.uid, [4, 1, 2, 3, 4, 5, 6]);
        assert_eq!(backup.pages.len(), 540);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_malformed_artifact_before_conversion() {
        let mut artifact = artifact();
        artifact.pages.pop();
        let error = artifact.into_device(3).unwrap_err();
        assert!(error.to_string().contains("pages"));
    }
}
