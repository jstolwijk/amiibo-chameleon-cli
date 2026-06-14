use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serialport::{ClearBuffer, SerialPort};

use crate::error::{Error, Result};
use crate::protocol::{self, Transport};

const GET_APP_VERSION: u16 = 1000;
const SET_ACTIVE_SLOT: u16 = 1003;
const SET_SLOT_TAG_TYPE: u16 = 1004;
const SET_SLOT_DATA_DEFAULT: u16 = 1005;
const SET_SLOT_ENABLE: u16 = 1006;
const SET_SLOT_TAG_NICK: u16 = 1007;
const GET_SLOT_TAG_NICK: u16 = 1008;
const SLOT_DATA_CONFIG_SAVE: u16 = 1009;
const GET_GIT_VERSION: u16 = 1017;
const GET_ACTIVE_SLOT: u16 = 1018;
const GET_SLOT_INFO: u16 = 1019;
const GET_ENABLED_SLOTS: u16 = 1023;
const HF14A_SET_ANTI_COLL_DATA: u16 = 4001;
const HF14A_GET_ANTI_COLL_DATA: u16 = 4018;
const MF0_NTAG_GET_UID_MAGIC_MODE: u16 = 4019;
const MF0_NTAG_SET_UID_MAGIC_MODE: u16 = 4020;
const MF0_NTAG_READ_EMU_PAGE_DATA: u16 = 4021;
const MF0_NTAG_WRITE_EMU_PAGE_DATA: u16 = 4022;
const MF0_NTAG_GET_VERSION_DATA: u16 = 4023;
const MF0_NTAG_GET_SIGNATURE_DATA: u16 = 4025;
const MF0_NTAG_GET_COUNTER_DATA: u16 = 4027;
const MF0_NTAG_GET_PAGE_COUNT: u16 = 4030;
const MF0_NTAG_GET_WRITE_MODE: u16 = 4031;
const MF0_NTAG_SET_WRITE_MODE: u16 = 4032;
const MF0_NTAG_GET_DETECTION_ENABLE: u16 = 4036;
const NTAG_215: u16 = 1101;
const STATUS_FLASH_READ_FAIL: u16 = 0x71;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub firmware_major: u8,
    pub firmware_minor: u8,
    pub git_version: String,
    pub active_slot: u8,
    pub slots: [SlotInfo; 8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotInfo {
    pub hf_type: TagType,
    pub lf_type: TagType,
    pub hf_enabled: bool,
    pub lf_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtagBackup {
    pub firmware_major: u8,
    pub firmware_minor: u8,
    pub git_version: String,
    pub slot: u8,
    pub hf_type: TagType,
    pub hf_enabled: bool,
    pub hf_nickname: String,
    pub lf_type: TagType,
    pub lf_enabled: bool,
    pub lf_nickname: String,
    pub uid: Vec<u8>,
    pub atqa: [u8; 2],
    pub sak: u8,
    pub ats: Vec<u8>,
    pub uid_magic: bool,
    pub write_mode: u8,
    pub detection_enabled: bool,
    pub counter: u32,
    pub counter_tearing: bool,
    pub version_data: Vec<u8>,
    pub signature_data: Vec<u8>,
    pub pages: Vec<u8>,
}

struct AntiCollision {
    uid: Vec<u8>,
    atqa: [u8; 2],
    sak: u8,
    ats: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagType(pub u16);

impl fmt::Display for TagType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            0 => "Undefined",
            100 => "EM410X",
            101 => "EM410X/16",
            102 => "EM410X/32",
            103 => "EM410X/64",
            104 => "EM410X Electra",
            150 => "PAC/Stanley",
            170 => "Viking",
            180 => "Jablotron",
            200 => "HIDProx",
            201 => "ioProx",
            310 => "IDTECK",
            1000 => "MIFARE Mini",
            1001 => "MIFARE Classic 1K",
            1002 => "MIFARE Classic 2K",
            1003 => "MIFARE Classic 4K",
            1100 => "NTAG 213",
            1101 => "NTAG 215",
            1102 => "NTAG 216",
            1103 => "MIFARE Ultralight",
            1104 => "MIFARE Ultralight C",
            1105 => "MIFARE Ultralight EV1 640-bit",
            1106 => "MIFARE Ultralight EV1 1312-bit",
            1107 => "NTAG 210",
            1108 => "NTAG 212",
            3000 => "ISO14443-4",
            value if (1..9).contains(&value) => "Legacy tag type",
            _ => return write!(formatter, "Unknown ({})", self.0),
        };
        formatter.write_str(name)
    }
}

pub struct Connection {
    path: PathBuf,
    transport: Box<dyn SerialPort>,
}

impl Connection {
    pub fn open(port: Option<&Path>) -> Result<Self> {
        if let Some(path) = port {
            return Self::open_path(path);
        }

        let mut matches = Vec::new();
        for path in candidate_ports()? {
            if let Ok(connection) = Self::open_path(&path) {
                matches.push(connection);
            }
        }

        match matches.len() {
            0 => Err(Error::DeviceNotFound),
            1 => Ok(matches.remove(0)),
            _ => Err(Error::MultipleDevices(
                matches.into_iter().map(|device| device.path).collect(),
            )),
        }
    }

    fn open_path(path: &Path) -> Result<Self> {
        let mut transport = serialport::new(path.to_string_lossy(), 115_200)
            .timeout(Duration::from_secs(3))
            .open()?;
        transport.write_data_terminal_ready(true)?;
        let _ = transport.clear(ClearBuffer::Input);

        let mut connection = Self {
            path: path.to_owned(),
            transport,
        };
        connection.app_version()?;
        Ok(connection)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn inspect(&mut self) -> Result<DeviceInfo> {
        inspect(self.transport.as_mut())
    }

    pub fn backup_ntag(&mut self, slot: u8) -> Result<NtagBackup> {
        backup_ntag(self.transport.as_mut(), slot)
    }

    pub fn flash_ntag215(
        &mut self,
        slot: u8,
        dump: &[u8],
        uid: [u8; 7],
        nickname: &str,
    ) -> Result<()> {
        flash_ntag215(self.transport.as_mut(), slot, dump, uid, nickname)
    }

    fn app_version(&mut self) -> Result<(u8, u8)> {
        app_version(self.transport.as_mut())
    }
}

fn inspect<T: Transport + ?Sized>(transport: &mut T) -> Result<DeviceInfo> {
    let (firmware_major, firmware_minor) = app_version(transport)?;
    if firmware_major != 2 {
        return Err(Error::UnsupportedFirmware {
            major: firmware_major,
            minor: firmware_minor,
        });
    }

    let git_version = command(transport, GET_GIT_VERSION, 1..=128)?;
    let git_version = String::from_utf8(git_version)
        .map_err(|_| Error::Protocol("Git version is not valid UTF-8".into()))?;
    let active = command(transport, GET_ACTIVE_SLOT, 1..=1)?[0];
    if active > 7 {
        return Err(Error::Protocol(format!(
            "active slot index {active} is outside 0..=7"
        )));
    }
    let types = command(transport, GET_SLOT_INFO, 32..=32)?;
    let enabled = command(transport, GET_ENABLED_SLOTS, 16..=16)?;

    let slots = std::array::from_fn(|index| {
        let type_offset = index * 4;
        let enabled_offset = index * 2;
        SlotInfo {
            hf_type: TagType(u16::from_be_bytes([
                types[type_offset],
                types[type_offset + 1],
            ])),
            lf_type: TagType(u16::from_be_bytes([
                types[type_offset + 2],
                types[type_offset + 3],
            ])),
            hf_enabled: enabled[enabled_offset] != 0,
            lf_enabled: enabled[enabled_offset + 1] != 0,
        }
    });

    Ok(DeviceInfo {
        firmware_major,
        firmware_minor,
        git_version,
        active_slot: active + 1,
        slots,
    })
}

fn flash_ntag215<T: Transport + ?Sized>(
    transport: &mut T,
    slot: u8,
    dump: &[u8],
    uid: [u8; 7],
    nickname: &str,
) -> Result<()> {
    if !(1..=8).contains(&slot) {
        return Err(Error::Protocol(format!("slot {slot} is outside 1..=8")));
    }
    if dump.len() != 540 {
        return Err(Error::Protocol(format!(
            "NTAG215 dump must be 540 bytes, found {}",
            dump.len()
        )));
    }
    if nickname.len() > 32 {
        return Err(Error::Protocol("slot nickname exceeds 32 bytes".into()));
    }

    let before = inspect(transport)?;
    let target = before.slots[usize::from(slot - 1)];
    if target.hf_type.0 != 0 && !is_ntag_family(target.hf_type) {
        return Err(Error::Protocol(format!(
            "slot {slot} contains {}; refusing to overwrite a non-NTAG HF slot",
            target.hf_type
        )));
    }
    let rollback = if is_ntag_family(target.hf_type) {
        Some(backup_ntag(transport, slot)?)
    } else {
        None
    };

    let result = program_ntag215(transport, slot, dump, uid, nickname);
    if let Err(error) = result {
        if let Some(backup) = rollback {
            if let Err(rollback_error) = restore_ntag(transport, &backup, before.active_slot) {
                return Err(Error::Protocol(format!(
                    "{error}; rollback also failed: {rollback_error}"
                )));
            }
        } else {
            let _ = command_with_payload(transport, SET_SLOT_ENABLE, &[slot - 1, 2, 0], 0..=0);
            let _ =
                command_with_payload(transport, SET_ACTIVE_SLOT, &[before.active_slot - 1], 0..=0);
        }
        return Err(error);
    }
    Ok(())
}

fn program_ntag215<T: Transport + ?Sized>(
    transport: &mut T,
    slot: u8,
    dump: &[u8],
    uid: [u8; 7],
    nickname: &str,
) -> Result<()> {
    let mut emulated_pages = dump.to_vec();
    apply_amiibo_auth_pages(&mut emulated_pages, uid)?;

    let mut type_payload = vec![slot - 1];
    type_payload.extend_from_slice(&NTAG_215.to_be_bytes());
    command_with_payload(transport, SET_SLOT_TAG_TYPE, &type_payload, 0..=0)?;
    command_with_payload(transport, SET_SLOT_DATA_DEFAULT, &type_payload, 0..=0)?;
    command_with_payload(transport, SET_ACTIVE_SLOT, &[slot - 1], 0..=0)?;

    write_pages(transport, &emulated_pages)?;
    set_anti_collision(transport, &uid, [0x44, 0x00], 0, &[])?;
    command_with_payload(transport, MF0_NTAG_SET_UID_MAGIC_MODE, &[0], 0..=0)?;
    command_with_payload(transport, MF0_NTAG_SET_WRITE_MODE, &[0], 0..=0)?;
    command_with_payload(
        transport,
        SET_SLOT_TAG_NICK,
        &[&[slot - 1, 2], nickname.as_bytes()].concat(),
        0..=0,
    )?;
    command_with_payload(transport, SET_SLOT_ENABLE, &[slot - 1, 2, 1], 0..=0)?;
    command(transport, SLOT_DATA_CONFIG_SAVE, 0..=0)?;

    verify_ntag215(transport, slot, &emulated_pages, uid, nickname)
}

fn apply_amiibo_auth_pages(pages: &mut [u8], uid: [u8; 7]) -> Result<()> {
    if pages.len() != 540 {
        return Err(Error::Protocol(format!(
            "NTAG215 memory must be 540 bytes, found {}",
            pages.len()
        )));
    }
    let password = [
        0xAA ^ uid[1] ^ uid[3],
        0x55 ^ uid[2] ^ uid[4],
        0xAA ^ uid[3] ^ uid[5],
        0x55 ^ uid[4] ^ uid[6],
    ];
    pages[532..536].copy_from_slice(&password);
    pages[536..540].copy_from_slice(&[0x80, 0x80, 0, 0]);
    Ok(())
}

fn write_pages<T: Transport + ?Sized>(transport: &mut T, pages: &[u8]) -> Result<()> {
    for (index, chunk) in pages.chunks(16 * 4).enumerate() {
        let page = u8::try_from(index * 16)
            .map_err(|_| Error::Protocol("page index exceeds protocol range".into()))?;
        let count = u8::try_from(chunk.len() / 4)
            .map_err(|_| Error::Protocol("page count exceeds protocol range".into()))?;
        let mut payload = vec![page, count];
        payload.extend_from_slice(chunk);
        command_with_payload(transport, MF0_NTAG_WRITE_EMU_PAGE_DATA, &payload, 0..=0)?;
    }
    Ok(())
}

fn set_anti_collision<T: Transport + ?Sized>(
    transport: &mut T,
    uid: &[u8],
    atqa: [u8; 2],
    sak: u8,
    ats: &[u8],
) -> Result<()> {
    let mut payload = vec![uid.len() as u8];
    payload.extend_from_slice(uid);
    payload.extend_from_slice(&atqa);
    payload.push(sak);
    payload.push(ats.len() as u8);
    payload.extend_from_slice(ats);
    command_with_payload(transport, HF14A_SET_ANTI_COLL_DATA, &payload, 0..=0)?;
    Ok(())
}

fn verify_ntag215<T: Transport + ?Sized>(
    transport: &mut T,
    slot: u8,
    dump: &[u8],
    uid: [u8; 7],
    nickname: &str,
) -> Result<()> {
    let page_count = command(transport, MF0_NTAG_GET_PAGE_COUNT, 1..=1)?[0];
    if page_count != 135 {
        return Err(Error::Protocol(format!(
            "NTAG215 emulator reports {page_count} pages, expected 135"
        )));
    }
    let mut read_back = Vec::with_capacity(540);
    let mut page = 0_u8;
    while page < 135 {
        let count = (135 - page).min(32);
        read_back.extend(command_with_payload(
            transport,
            MF0_NTAG_READ_EMU_PAGE_DATA,
            &[page, count],
            usize::from(count) * 4..=usize::from(count) * 4,
        )?);
        page += count;
    }
    if read_back != dump {
        return Err(Error::Protocol("NTAG215 page read-back mismatch".into()));
    }

    let anti = parse_anti_collision(&command(transport, HF14A_GET_ANTI_COLL_DATA, 5..=265)?)?;
    if anti.uid != uid || anti.atqa != [0x44, 0] || anti.sak != 0 || !anti.ats.is_empty() {
        return Err(Error::Protocol(
            "anti-collision read-back does not match Amiibo settings".into(),
        ));
    }
    if bool_value(command(transport, MF0_NTAG_GET_UID_MAGIC_MODE, 1..=1)?[0])? {
        return Err(Error::Protocol("UID-magic mode remained enabled".into()));
    }
    if command(transport, MF0_NTAG_GET_WRITE_MODE, 1..=1)?[0] != 0 {
        return Err(Error::Protocol("write mode is not NORMAL".into()));
    }
    let info = inspect(transport)?;
    let target = info.slots[usize::from(slot - 1)];
    if target.hf_type != TagType(NTAG_215) || !target.hf_enabled {
        return Err(Error::Protocol(
            "slot type or HF enablement verification failed".into(),
        ));
    }
    let actual_nickname =
        command_with_payload(transport, GET_SLOT_TAG_NICK, &[slot - 1, 2], 0..=32)?;
    if actual_nickname != nickname.as_bytes() {
        return Err(Error::Protocol("slot nickname verification failed".into()));
    }
    Ok(())
}

fn restore_ntag<T: Transport + ?Sized>(
    transport: &mut T,
    backup: &NtagBackup,
    active_slot: u8,
) -> Result<()> {
    let mut type_payload = vec![backup.slot - 1];
    type_payload.extend_from_slice(&backup.hf_type.0.to_be_bytes());
    command_with_payload(transport, SET_SLOT_TAG_TYPE, &type_payload, 0..=0)?;
    command_with_payload(transport, SET_SLOT_DATA_DEFAULT, &type_payload, 0..=0)?;
    command_with_payload(transport, SET_ACTIVE_SLOT, &[backup.slot - 1], 0..=0)?;
    write_pages(transport, &backup.pages)?;
    set_anti_collision(transport, &backup.uid, backup.atqa, backup.sak, &backup.ats)?;
    command_with_payload(
        transport,
        MF0_NTAG_SET_UID_MAGIC_MODE,
        &[u8::from(backup.uid_magic)],
        0..=0,
    )?;
    command_with_payload(
        transport,
        MF0_NTAG_SET_WRITE_MODE,
        &[backup.write_mode],
        0..=0,
    )?;
    command_with_payload(
        transport,
        SET_SLOT_TAG_NICK,
        &[&[backup.slot - 1, 2], backup.hf_nickname.as_bytes()].concat(),
        0..=0,
    )?;
    command_with_payload(
        transport,
        SET_SLOT_ENABLE,
        &[backup.slot - 1, 2, u8::from(backup.hf_enabled)],
        0..=0,
    )?;
    command(transport, SLOT_DATA_CONFIG_SAVE, 0..=0)?;
    command_with_payload(transport, SET_ACTIVE_SLOT, &[active_slot - 1], 0..=0)?;
    Ok(())
}

fn backup_ntag<T: Transport + ?Sized>(transport: &mut T, slot: u8) -> Result<NtagBackup> {
    if !(1..=8).contains(&slot) {
        return Err(Error::Protocol(format!("slot {slot} is outside 1..=8")));
    }

    let info = inspect(transport)?;
    let slot_info = info.slots[usize::from(slot - 1)];
    if !is_ntag_family(slot_info.hf_type) {
        return Err(Error::Protocol(format!(
            "slot {slot} is {}, not an NTAG/MIFARE Ultralight slot",
            slot_info.hf_type
        )));
    }

    let hf_nickname = read_nickname(transport, slot, 2)?;
    let lf_nickname = read_nickname(transport, slot, 1)?;

    command_with_payload(transport, SET_ACTIVE_SLOT, &[slot - 1], 0..=0)?;
    let result =
        read_active_ntag_state(transport, &info, slot, slot_info, hf_nickname, lf_nickname);
    let restore = command_with_payload(transport, SET_ACTIVE_SLOT, &[info.active_slot - 1], 0..=0);

    match (result, restore) {
        (Ok(backup), Ok(_)) => Ok(backup),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}

fn read_nickname<T: Transport + ?Sized>(
    transport: &mut T,
    slot: u8,
    sense_type: u8,
) -> Result<String> {
    let response = protocol::transact_raw(transport, GET_SLOT_TAG_NICK, &[slot - 1, sense_type])?;
    let bytes = match response.status {
        protocol::STATUS_SUCCESS => response.payload,
        STATUS_FLASH_READ_FAIL => Vec::new(),
        status => {
            return Err(Error::Protocol(format!(
                "command {GET_SLOT_TAG_NICK} failed with status 0x{status:04X}"
            )));
        }
    };
    if bytes.len() > 32 {
        return Err(Error::Protocol(format!(
            "slot nickname is {} bytes, expected at most 32",
            bytes.len()
        )));
    }
    String::from_utf8(bytes).map_err(|_| Error::Protocol("slot nickname is not valid UTF-8".into()))
}

fn read_active_ntag_state<T: Transport + ?Sized>(
    transport: &mut T,
    info: &DeviceInfo,
    slot: u8,
    slot_info: SlotInfo,
    hf_nickname: String,
    lf_nickname: String,
) -> Result<NtagBackup> {
    let anti_coll = command(transport, HF14A_GET_ANTI_COLL_DATA, 5..=265)?;
    let anti_coll = parse_anti_collision(&anti_coll)?;
    let uid_magic = bool_value(command(transport, MF0_NTAG_GET_UID_MAGIC_MODE, 1..=1)?[0])?;
    let write_mode = command(transport, MF0_NTAG_GET_WRITE_MODE, 1..=1)?[0];
    if write_mode > 4 {
        return Err(Error::Protocol(format!(
            "invalid NTAG write mode {write_mode}"
        )));
    }
    let detection_enabled =
        bool_value(command(transport, MF0_NTAG_GET_DETECTION_ENABLE, 1..=1)?[0])?;
    let version_data = command(transport, MF0_NTAG_GET_VERSION_DATA, 8..=8)?;
    let signature_data = command(transport, MF0_NTAG_GET_SIGNATURE_DATA, 32..=32)?;
    let counter_data = command_with_payload(transport, MF0_NTAG_GET_COUNTER_DATA, &[0], 4..=4)?;
    let counter = u32::from(counter_data[0])
        | (u32::from(counter_data[1]) << 8)
        | (u32::from(counter_data[2]) << 16);
    let counter_tearing = match counter_data[3] {
        0xBD => false,
        0x00 => true,
        value => {
            return Err(Error::Protocol(format!(
                "invalid NTAG counter tearing byte 0x{value:02X}"
            )));
        }
    };
    let page_count = command(transport, MF0_NTAG_GET_PAGE_COUNT, 1..=1)?[0];
    let mut pages = Vec::with_capacity(usize::from(page_count) * 4);
    let mut page = 0_u8;
    while page < page_count {
        let count = (page_count - page).min(32);
        let data = command_with_payload(
            transport,
            MF0_NTAG_READ_EMU_PAGE_DATA,
            &[page, count],
            usize::from(count) * 4..=usize::from(count) * 4,
        )?;
        pages.extend_from_slice(&data);
        page += count;
    }

    Ok(NtagBackup {
        firmware_major: info.firmware_major,
        firmware_minor: info.firmware_minor,
        git_version: info.git_version.clone(),
        slot,
        hf_type: slot_info.hf_type,
        hf_enabled: slot_info.hf_enabled,
        hf_nickname,
        lf_type: slot_info.lf_type,
        lf_enabled: slot_info.lf_enabled,
        lf_nickname,
        uid: anti_coll.uid,
        atqa: anti_coll.atqa,
        sak: anti_coll.sak,
        ats: anti_coll.ats,
        uid_magic,
        write_mode,
        detection_enabled,
        counter,
        counter_tearing,
        version_data,
        signature_data,
        pages,
    })
}

fn parse_anti_collision(payload: &[u8]) -> Result<AntiCollision> {
    let uid_length = usize::from(payload[0]);
    if !matches!(uid_length, 4 | 7 | 10) {
        return Err(Error::Protocol(format!(
            "invalid anti-collision UID length {uid_length}"
        )));
    }
    let fixed_end = 1 + uid_length + 2 + 1 + 1;
    if payload.len() < fixed_end {
        return Err(Error::Protocol("truncated anti-collision response".into()));
    }
    let ats_length = usize::from(payload[fixed_end - 1]);
    if payload.len() != fixed_end + ats_length {
        return Err(Error::Protocol("invalid anti-collision ATS length".into()));
    }
    Ok(AntiCollision {
        uid: payload[1..1 + uid_length].to_vec(),
        atqa: [payload[1 + uid_length], payload[2 + uid_length]],
        sak: payload[3 + uid_length],
        ats: payload[fixed_end..].to_vec(),
    })
}

fn bool_value(value: u8) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::Protocol(format!("invalid boolean value {value}"))),
    }
}

fn is_ntag_family(tag_type: TagType) -> bool {
    matches!(tag_type.0, 1100..=1108)
}

fn app_version<T: Transport + ?Sized>(transport: &mut T) -> Result<(u8, u8)> {
    let payload = command(transport, GET_APP_VERSION, 2..=2)?;
    Ok((payload[0], payload[1]))
}

fn command<T: Transport + ?Sized>(
    transport: &mut T,
    command: u16,
    expected_length: std::ops::RangeInclusive<usize>,
) -> Result<Vec<u8>> {
    command_with_payload(transport, command, &[], expected_length)
}

fn command_with_payload<T: Transport + ?Sized>(
    transport: &mut T,
    command: u16,
    payload: &[u8],
    expected_length: std::ops::RangeInclusive<usize>,
) -> Result<Vec<u8>> {
    let response = protocol::transact(transport, command, payload)?;
    if !expected_length.contains(&response.payload.len()) {
        return Err(Error::Protocol(format!(
            "command {command} returned {} bytes, expected {}..={}",
            response.payload.len(),
            expected_length.start(),
            expected_length.end()
        )));
    }
    Ok(response.payload)
}

fn candidate_ports() -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir("/dev")? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("cu.usbmodem")
            || name.starts_with("cu.usbserial")
            || name.starts_with("cu.wchusbserial")
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Write};

    use super::{
        GET_ACTIVE_SLOT, GET_APP_VERSION, GET_ENABLED_SLOTS, GET_GIT_VERSION, GET_SLOT_INFO,
        GET_SLOT_TAG_NICK, HF14A_GET_ANTI_COLL_DATA, HF14A_SET_ANTI_COLL_DATA,
        MF0_NTAG_GET_COUNTER_DATA, MF0_NTAG_GET_DETECTION_ENABLE, MF0_NTAG_GET_PAGE_COUNT,
        MF0_NTAG_GET_SIGNATURE_DATA, MF0_NTAG_GET_UID_MAGIC_MODE, MF0_NTAG_GET_VERSION_DATA,
        MF0_NTAG_GET_WRITE_MODE, MF0_NTAG_READ_EMU_PAGE_DATA, MF0_NTAG_SET_UID_MAGIC_MODE,
        MF0_NTAG_SET_WRITE_MODE, MF0_NTAG_WRITE_EMU_PAGE_DATA, SET_ACTIVE_SLOT,
        SET_SLOT_DATA_DEFAULT, SET_SLOT_ENABLE, SET_SLOT_TAG_NICK, SET_SLOT_TAG_TYPE,
        SLOT_DATA_CONFIG_SAVE, TagType, apply_amiibo_auth_pages, backup_ntag, flash_ntag215,
        inspect,
    };
    use crate::protocol;

    #[test]
    fn parses_complete_device_state() {
        let mut types = vec![0; 32];
        types[0..2].copy_from_slice(&1101_u16.to_be_bytes());
        types[4..6].copy_from_slice(&1001_u16.to_be_bytes());
        let mut enabled = vec![0; 16];
        enabled[0] = 1;

        let responses = [
            response(GET_APP_VERSION, &[2, 1]),
            response(GET_GIT_VERSION, b"v2.1.0"),
            response(GET_ACTIVE_SLOT, &[0]),
            response(GET_SLOT_INFO, &types),
            response(GET_ENABLED_SLOTS, &enabled),
        ]
        .concat();
        let mut transport = FakeTransport::new(responses);

        let info = inspect(&mut transport).unwrap();
        assert_eq!((info.firmware_major, info.firmware_minor), (2, 1));
        assert_eq!(info.git_version, "v2.1.0");
        assert_eq!(info.active_slot, 1);
        assert_eq!(info.slots[0].hf_type, TagType(1101));
        assert!(info.slots[0].hf_enabled);
        assert_eq!(info.slots[1].hf_type, TagType(1001));
        assert_eq!(transport.writes.len(), 5 * 10);
    }

    #[test]
    fn rejects_incompatible_firmware_major() {
        let mut transport = FakeTransport::new(response(GET_APP_VERSION, &[3, 0]));
        let error = inspect(&mut transport).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported Chameleon firmware 3.0")
        );
    }

    #[test]
    fn formats_known_and_unknown_tag_types() {
        assert_eq!(TagType(1101).to_string(), "NTAG 215");
        assert_eq!(TagType(65000).to_string(), "Unknown (65000)");
    }

    #[test]
    fn backs_up_complete_ntag_state_and_restores_active_slot() {
        let mut types = vec![0; 32];
        types[4..6].copy_from_slice(&1101_u16.to_be_bytes());
        types[6..8].copy_from_slice(&100_u16.to_be_bytes());
        let mut enabled = vec![0; 16];
        enabled[2] = 1;
        enabled[3] = 1;

        let pages = vec![0xA5; 135 * 4];
        let mut responses = [
            response(GET_APP_VERSION, &[2, 1]),
            response(GET_GIT_VERSION, b"v2.1.0"),
            response(GET_ACTIVE_SLOT, &[6]),
            response(GET_SLOT_INFO, &types),
            response(GET_ENABLED_SLOTS, &enabled),
            response(GET_SLOT_TAG_NICK, b"Mario"),
            response(GET_SLOT_TAG_NICK, b"Badge"),
            response(SET_ACTIVE_SLOT, &[]),
            response(
                HF14A_GET_ANTI_COLL_DATA,
                &[7, 4, 1, 2, 3, 4, 5, 6, 0x44, 0, 0, 0],
            ),
            response(MF0_NTAG_GET_UID_MAGIC_MODE, &[0]),
            response(MF0_NTAG_GET_WRITE_MODE, &[0]),
            response(MF0_NTAG_GET_DETECTION_ENABLE, &[1]),
            response(MF0_NTAG_GET_VERSION_DATA, &[0; 8]),
            response(MF0_NTAG_GET_SIGNATURE_DATA, &[0; 32]),
            response(MF0_NTAG_GET_COUNTER_DATA, &[0x56, 0x34, 0x12, 0xBD]),
            response(MF0_NTAG_GET_PAGE_COUNT, &[135]),
        ]
        .concat();
        for chunk in pages.chunks(32 * 4) {
            responses.extend(response(MF0_NTAG_READ_EMU_PAGE_DATA, chunk));
        }
        responses.extend(response(SET_ACTIVE_SLOT, &[]));
        let mut transport = FakeTransport::new(responses);

        let backup = backup_ntag(&mut transport, 2).unwrap();
        assert_eq!(backup.slot, 2);
        assert_eq!(backup.hf_type, TagType(1101));
        assert_eq!(backup.lf_type, TagType(100));
        assert_eq!(backup.hf_nickname, "Mario");
        assert_eq!(backup.lf_nickname, "Badge");
        assert_eq!(backup.uid, [4, 1, 2, 3, 4, 5, 6]);
        assert_eq!(backup.counter, 0x123456);
        assert_eq!(backup.pages, pages);

        let requests = request_frames(&transport.writes);
        assert_eq!(requests.last(), Some(&(SET_ACTIVE_SLOT, vec![6])));
        let page_reads: Vec<_> = requests
            .iter()
            .filter(|(command, _)| *command == MF0_NTAG_READ_EMU_PAGE_DATA)
            .collect();
        assert_eq!(page_reads.len(), 5);
        assert_eq!(page_reads[0].1, [0, 32]);
        assert_eq!(page_reads[4].1, [128, 7]);
    }

    #[test]
    fn backup_accepts_missing_nickname_records() {
        let mut types = vec![0; 32];
        types[20..22].copy_from_slice(&1101_u16.to_be_bytes());
        let mut enabled = vec![0; 16];
        enabled[10] = 1;
        let pages = vec![0; 135 * 4];

        let mut responses = [
            response(GET_APP_VERSION, &[2, 1]),
            response(GET_GIT_VERSION, b"v2.1.0"),
            response(GET_ACTIVE_SLOT, &[6]),
            response(GET_SLOT_INFO, &types),
            response(GET_ENABLED_SLOTS, &enabled),
            response_with_status(GET_SLOT_TAG_NICK, 0x71, &[]),
            response_with_status(GET_SLOT_TAG_NICK, 0x71, &[]),
            response(SET_ACTIVE_SLOT, &[]),
            response(
                HF14A_GET_ANTI_COLL_DATA,
                &[7, 4, 1, 2, 3, 4, 5, 6, 0x44, 0, 0, 0],
            ),
            response(MF0_NTAG_GET_UID_MAGIC_MODE, &[0]),
            response(MF0_NTAG_GET_WRITE_MODE, &[0]),
            response(MF0_NTAG_GET_DETECTION_ENABLE, &[0]),
            response(MF0_NTAG_GET_VERSION_DATA, &[0; 8]),
            response(MF0_NTAG_GET_SIGNATURE_DATA, &[0; 32]),
            response(MF0_NTAG_GET_COUNTER_DATA, &[0, 0, 0, 0xBD]),
            response(MF0_NTAG_GET_PAGE_COUNT, &[135]),
        ]
        .concat();
        for chunk in pages.chunks(32 * 4) {
            responses.extend(response(MF0_NTAG_READ_EMU_PAGE_DATA, chunk));
        }
        responses.extend(response(SET_ACTIVE_SLOT, &[]));
        let mut transport = FakeTransport::new(responses);

        let backup = backup_ntag(&mut transport, 6).unwrap();
        assert!(backup.hf_nickname.is_empty());
        assert!(backup.lf_nickname.is_empty());
    }

    #[test]
    fn flashes_and_verifies_ntag215_in_empty_slot() {
        let initial_types = vec![0; 32];
        let initial_enabled = vec![0; 16];
        let mut final_types = initial_types.clone();
        final_types[20..22].copy_from_slice(&1101_u16.to_be_bytes());
        let mut final_enabled = initial_enabled.clone();
        final_enabled[10] = 1;
        let dump = vec![0x5A; 540];
        let uid = [4, 1, 2, 3, 4, 5, 6];
        let mut expected_pages = dump.clone();
        apply_amiibo_auth_pages(&mut expected_pages, uid).unwrap();
        let anti = [7, 4, 1, 2, 3, 4, 5, 6, 0x44, 0, 0, 0];

        let mut responses = [
            response(GET_APP_VERSION, &[2, 1]),
            response(GET_GIT_VERSION, b"v2.1.0"),
            response(GET_ACTIVE_SLOT, &[6]),
            response(GET_SLOT_INFO, &initial_types),
            response(GET_ENABLED_SLOTS, &initial_enabled),
            response(SET_SLOT_TAG_TYPE, &[]),
            response(SET_SLOT_DATA_DEFAULT, &[]),
            response(SET_ACTIVE_SLOT, &[]),
        ]
        .concat();
        for _ in dump.chunks(16 * 4) {
            responses.extend(response(MF0_NTAG_WRITE_EMU_PAGE_DATA, &[]));
        }
        responses.extend(
            [
                response(HF14A_SET_ANTI_COLL_DATA, &[]),
                response(MF0_NTAG_SET_UID_MAGIC_MODE, &[]),
                response(MF0_NTAG_SET_WRITE_MODE, &[]),
                response(SET_SLOT_TAG_NICK, &[]),
                response(SET_SLOT_ENABLE, &[]),
                response(SLOT_DATA_CONFIG_SAVE, &[]),
                response(MF0_NTAG_GET_PAGE_COUNT, &[135]),
            ]
            .concat(),
        );
        for chunk in expected_pages.chunks(32 * 4) {
            responses.extend(response(MF0_NTAG_READ_EMU_PAGE_DATA, chunk));
        }
        responses.extend(
            [
                response(HF14A_GET_ANTI_COLL_DATA, &anti),
                response(MF0_NTAG_GET_UID_MAGIC_MODE, &[0]),
                response(MF0_NTAG_GET_WRITE_MODE, &[0]),
                response(GET_APP_VERSION, &[2, 1]),
                response(GET_GIT_VERSION, b"v2.1.0"),
                response(GET_ACTIVE_SLOT, &[5]),
                response(GET_SLOT_INFO, &final_types),
                response(GET_ENABLED_SLOTS, &final_enabled),
                response(GET_SLOT_TAG_NICK, b"Mario"),
            ]
            .concat(),
        );
        let mut transport = FakeTransport::new(responses);

        flash_ntag215(&mut transport, 6, &dump, uid, "Mario").unwrap();

        let requests = request_frames(&transport.writes);
        let writes: Vec<_> = requests
            .iter()
            .filter(|(command, _)| *command == MF0_NTAG_WRITE_EMU_PAGE_DATA)
            .collect();
        assert_eq!(writes.len(), 9);
        assert_eq!(writes[0].1[0..2], [0, 16]);
        assert_eq!(writes[8].1[0..2], [128, 7]);
        assert_eq!(&writes[8].1[22..26], &[0xA8, 0x53, 0xAC, 0x57]);
        assert_eq!(&writes[8].1[26..30], &[0x80, 0x80, 0, 0]);
        assert_eq!(
            requests
                .iter()
                .find(|(command, _)| *command == HF14A_SET_ANTI_COLL_DATA)
                .unwrap()
                .1,
            anti
        );
    }

    #[test]
    fn derives_official_amiibo_password_and_pack() {
        let mut pages = vec![0; 540];
        apply_amiibo_auth_pages(&mut pages, [0x04, 0x64, 0x7F, 0x62, 0x98, 0x3C, 0x80]).unwrap();
        assert_eq!(&pages[532..536], &[0xAC, 0xB2, 0xF4, 0x4D]);
        assert_eq!(&pages[536..540], &[0x80, 0x80, 0, 0]);
    }

    fn response(command: u16, payload: &[u8]) -> Vec<u8> {
        response_with_status(command, protocol::STATUS_SUCCESS, payload)
    }

    fn response_with_status(command: u16, status: u16, payload: &[u8]) -> Vec<u8> {
        let request = protocol::encode(command, payload).unwrap();
        let mut frame = request;
        frame[4..6].copy_from_slice(&status.to_be_bytes());
        frame[8] = lrc(&frame[..8]);
        let last = frame.len() - 1;
        frame[last] = lrc(&frame[..last]);
        frame
    }

    fn lrc(bytes: &[u8]) -> u8 {
        0_u8.wrapping_sub(bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)))
    }

    fn request_frames(bytes: &[u8]) -> Vec<(u16, Vec<u8>)> {
        let mut frames = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let length = usize::from(u16::from_be_bytes([bytes[offset + 6], bytes[offset + 7]]));
            let end = offset + 10 + length;
            frames.push((
                u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]),
                bytes[offset + 9..offset + 9 + length].to_vec(),
            ));
            offset = end;
        }
        frames
    }

    struct FakeTransport {
        reads: Cursor<Vec<u8>>,
        writes: Vec<u8>,
    }

    impl FakeTransport {
        fn new(reads: Vec<u8>) -> Self {
            Self {
                reads: Cursor::new(reads),
                writes: Vec::new(),
            }
        }
    }

    impl Read for FakeTransport {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.reads.read(buffer)
        }
    }

    impl Write for FakeTransport {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.writes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
