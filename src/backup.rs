use std::fs::OpenOptions;
use std::io::BufWriter;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::device::NtagBackup;
use crate::error::Result;

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
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use crate::device::{NtagBackup, TagType};

    use super::BackupArtifact;

    #[test]
    fn serializes_binary_state_as_hex() {
        let artifact = BackupArtifact::from_device(NtagBackup {
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
            uid: vec![0x04, 0xAA],
            atqa: [0x44, 0],
            sak: 0,
            ats: vec![],
            uid_magic: false,
            write_mode: 0,
            detection_enabled: false,
            counter: 0x123456,
            counter_tearing: false,
            version_data: vec![1, 2],
            signature_data: vec![0xAB, 0xCD],
            pages: vec![0, 0xFF],
        });
        let json = serde_json::to_string(&artifact).unwrap();
        assert!(json.contains("\"uid\":\"04AA\""));
        assert!(json.contains("\"pages\":\"00FF\""));
    }
}
