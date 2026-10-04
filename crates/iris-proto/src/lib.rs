//! Packet building and parsing for the EVision V1 protocol spoken by the
//! Tecware Phantom RGB (`320f:5064`).
//!
//! This crate performs no I/O. It owns the command allow-list as a closed
//! enum: no API here accepts an arbitrary command byte (D-005, AV-001).
//! Write requests can only be built through typed constructors, so a caller
//! cannot address bytes whose meaning PROTOCOL.md does not record.
//! PROTOCOL.md is the specification; every constant in this crate should be
//! traceable to a tagged fact there.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod address;
mod capability;
pub mod capture;
mod command;
mod config;
mod error;
#[cfg(test)]
mod fixtures;
mod leds;
mod packet;

pub use address::{
    COLOUR_SLOTS, COLOUR_STRIDE, CONFIG_STRIDE, Mode, Parameter, PollingRate, Profile, Rgb,
    colour_address, config_address,
};
pub use capability::{CAPABILITY_MAGIC, CAPABILITY_SIZE, Capabilities};
pub use command::Command;
pub use config::{ConfigBlock, Setting};
pub use error::Error;
pub use leds::LedMap;
pub use packet::{
    HEADER_LEN, MAX_COLOUR_PAYLOAD, MAX_PAYLOAD, PACKET_LEN, READ_CONFIG_SIZE, REPORT_ID, Reply,
    Request, checksum,
};

/// Result type used throughout this crate.
pub type Result<T> = std::result::Result<T, Error>;
