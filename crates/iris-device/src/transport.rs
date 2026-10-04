//! The transport seam (ARCHITECTURE.md §Testing seam).

use std::fmt;
use std::io;
use std::time::Duration;

use iris_proto::Request;

/// A channel to the keyboard's vendor interface.
///
/// Implemented by the hidraw node and by the simulated keyboard. There is
/// deliberately no method that writes raw bytes.
pub trait Transport {
    /// Writes one request (64 bytes on the wire).
    fn send(&mut self, request: &Request) -> Result<(), TransportError>;

    /// Reads the next input report of any report ID, waiting at most
    /// `timeout`. Returns `Ok(None)` on timeout. Key presses and media keys
    /// arrive here too and must be filtered by the caller (AV-008).
    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError>;
}

/// A transport failure.
#[derive(Debug)]
pub enum TransportError {
    /// The device has gone: unplugged, or renumbered after resume (AV-014).
    Disconnected,
    /// Any other I/O failure.
    Io(io::Error),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Disconnected => write!(f, "keyboard disconnected"),
            TransportError::Io(error) => write!(f, "I/O error: {error}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransportError::Disconnected => None,
            TransportError::Io(error) => Some(error),
        }
    }
}

impl From<io::Error> for TransportError {
    fn from(error: io::Error) -> TransportError {
        TransportError::Io(error)
    }
}
