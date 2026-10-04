use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::TransportError;

/// Why a device operation failed. Each variant is meant to map to distinct
/// guidance for the user (ARCHITECTURE.md §Cross-cutting concerns).
#[derive(Debug)]
pub enum DeviceError {
    /// No hidraw node exposes the keyboard's vendor channel.
    NotFound,
    /// The node exists but is not accessible: the host udev rule is missing
    /// or the keyboard was not replugged after adding it (F-025, AV-010).
    /// [`crate::permission_help`] gives the fix.
    PermissionDenied { node: PathBuf },
    /// Another process holds the node's advisory lock (AV-009).
    Busy { node: PathBuf },
    /// The device is not one Iris recognises well enough to talk to at all
    /// (wrong capability magic, for example a V2 board).
    IdentityMismatch(String),
    /// The device was recognised for reading only; writes are refused until
    /// its identity is verified (AV-007).
    WritesNotPermitted(String),
    /// No matching reply arrived in time.
    Timeout,
    /// The device has gone; rediscover it (AV-014).
    Disconnected,
    /// A write was acknowledged but reading back showed the keyboard does
    /// not hold what was written.
    ReadBackMismatch(String),
    /// A reply could not be interpreted.
    Protocol(iris_proto::Error),
    /// Any other I/O failure.
    Io(io::Error),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::NotFound => {
                write!(f, "no Tecware Phantom (320f:5064) vendor interface found")
            }
            DeviceError::PermissionDenied { node } => {
                write!(f, "permission denied opening {}", node.display())
            }
            DeviceError::Busy { node } => {
                write!(f, "{} is in use by another process", node.display())
            }
            DeviceError::IdentityMismatch(why) => write!(f, "unrecognised device: {why}"),
            DeviceError::WritesNotPermitted(why) => write!(f, "writes refused: {why}"),
            DeviceError::Timeout => write!(f, "no reply from the keyboard"),
            DeviceError::Disconnected => write!(f, "keyboard disconnected"),
            DeviceError::ReadBackMismatch(what) => {
                write!(f, "write not confirmed by read-back: {what}")
            }
            DeviceError::Protocol(error) => write!(f, "protocol error: {error}"),
            DeviceError::Io(error) => write!(f, "I/O error: {error}"),
        }
    }
}

impl std::error::Error for DeviceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DeviceError::Protocol(error) => Some(error),
            DeviceError::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TransportError> for DeviceError {
    fn from(error: TransportError) -> DeviceError {
        match error {
            TransportError::Disconnected => DeviceError::Disconnected,
            TransportError::Io(error) => DeviceError::Io(error),
        }
    }
}

impl From<iris_proto::Error> for DeviceError {
    fn from(error: iris_proto::Error) -> DeviceError {
        DeviceError::Protocol(error)
    }
}
