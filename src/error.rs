use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    CacheUnavailable,
    DatabaseMissing(PathBuf),
    DeviceBusy(PathBuf),
    DeviceNotFound,
    MultipleDevices(Vec<PathBuf>),
    GitFailed(String),
    InvalidDatabase(PathBuf),
    InvalidBackup(String),
    InvalidDump {
        path: PathBuf,
        reason: String,
    },
    Selection(String),
    Io(io::Error),
    Protocol(String),
    RecoveryPreserved {
        source: Box<Error>,
        path: PathBuf,
    },
    RollbackFailed {
        operation: Box<Error>,
        rollback: Box<Error>,
    },
    Json(serde_json::Error),
    Serial(serialport::Error),
    UnsupportedFirmware {
        major: u8,
        minor: u8,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheUnavailable => {
                write!(
                    formatter,
                    "could not determine the macOS user cache directory"
                )
            }
            Self::DatabaseMissing(path) => write!(
                formatter,
                "AmiiboDB cache does not exist at {}; run `amiibo database update` first",
                path.display()
            ),
            Self::DeviceBusy(path) => write!(
                formatter,
                "Chameleon at {} is already in use by another amiibo process",
                path.display()
            ),
            Self::DeviceNotFound => write!(
                formatter,
                "no Chameleon Ultra found; connect it over USB or pass `--port /dev/cu.*`"
            ),
            Self::MultipleDevices(paths) => write!(
                formatter,
                "multiple Chameleon devices responded; select one with `--port`: {}",
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::GitFailed(message) => write!(formatter, "Git operation failed: {message}"),
            Self::InvalidDatabase(path) => {
                write!(formatter, "invalid AmiiboDB checkout at {}", path.display())
            }
            Self::InvalidBackup(reason) => write!(formatter, "invalid backup artifact: {reason}"),
            Self::InvalidDump { path, reason } => {
                write!(formatter, "invalid dump {}: {reason}", path.display())
            }
            Self::Selection(message) => formatter.write_str(message),
            Self::Io(error) => error.fmt(formatter),
            Self::Protocol(message) => write!(formatter, "device protocol error: {message}"),
            Self::RecoveryPreserved { source, path } => write!(
                formatter,
                "{source}; recovery artifact retained at {}",
                path.display()
            ),
            Self::RollbackFailed {
                operation,
                rollback,
            } => write!(formatter, "{operation}; rollback also failed: {rollback}"),
            Self::Json(error) => write!(formatter, "backup format error: {error}"),
            Self::Serial(error) => write!(formatter, "serial error: {error}"),
            Self::UnsupportedFirmware { major, minor } => write!(
                formatter,
                "unsupported Chameleon firmware {major}.{minor}; this version requires firmware major version 2"
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serialport::Error> for Error {
    fn from(value: serialport::Error) -> Self {
        Self::Serial(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
