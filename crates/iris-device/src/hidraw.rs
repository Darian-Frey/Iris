//! The hidraw node as a [`Transport`] (D-003).
//!
//! Interface 1 has no OUT endpoint; the kernel turns `write()` into a
//! SET_REPORT control transfer, so a plain write is correct (PROTOCOL.md §1).

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use iris_proto::{PACKET_LEN, Request};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{FlockOperation, flock};
use rustix::io::Errno;

use crate::{DeviceError, Transport, TransportError};

/// Largest input report on the node (the vendor report, ID included).
const MAX_REPORT: usize = PACKET_LEN;

/// An open hidraw node, exclusively locked against other cooperating
/// processes (AV-009). The lock is advisory: OpenRGB does not take it.
#[derive(Debug)]
pub struct Hidraw {
    file: File,
    node: PathBuf,
}

impl Hidraw {
    /// Opens and locks a hidraw node.
    pub fn open(node: &Path) -> Result<Hidraw, DeviceError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(node)
            .map_err(|error| match error.kind() {
                io::ErrorKind::PermissionDenied => DeviceError::PermissionDenied {
                    node: node.to_path_buf(),
                },
                io::ErrorKind::NotFound => DeviceError::Disconnected,
                _ => DeviceError::Io(error),
            })?;
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(Errno::WOULDBLOCK) => {
                return Err(DeviceError::Busy {
                    node: node.to_path_buf(),
                });
            }
            Err(errno) => return Err(DeviceError::Io(errno.into())),
        }
        Ok(Hidraw {
            file,
            node: node.to_path_buf(),
        })
    }

    /// The node this transport was opened on.
    pub fn node(&self) -> &Path {
        &self.node
    }
}

/// hidraw reports a removed device as `ENODEV`; treat `ENXIO` the same.
fn map_io(error: io::Error) -> TransportError {
    match Errno::from_io_error(&error) {
        Some(Errno::NODEV | Errno::NXIO) => TransportError::Disconnected,
        _ => TransportError::Io(error),
    }
}

impl Transport for Hidraw {
    fn send(&mut self, request: &Request) -> Result<(), TransportError> {
        let packet = request.encode();
        let written = self.file.write(&packet).map_err(map_io)?;
        if written != packet.len() {
            return Err(TransportError::Io(io::Error::new(
                io::ErrorKind::WriteZero,
                format!("short write: {written} of {} bytes", packet.len()),
            )));
        }
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
        let timespec = Timespec::try_from(timeout).map_err(|_| {
            TransportError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "timeout too large",
            ))
        })?;
        let mut fds = [PollFd::new(&self.file, PollFlags::IN)];
        loop {
            match poll(&mut fds, Some(&timespec)) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(Errno::INTR) => continue,
                Err(errno) => return Err(map_io(errno.into())),
            }
        }
        let revents = fds[0].revents();
        if revents.intersects(PollFlags::HUP | PollFlags::ERR | PollFlags::NVAL) {
            return Err(TransportError::Disconnected);
        }
        let mut buffer = [0u8; MAX_REPORT];
        let read = self.file.read(&mut buffer).map_err(map_io)?;
        if read == 0 {
            return Err(TransportError::Disconnected);
        }
        Ok(Some(buffer[..read].to_vec()))
    }
}
