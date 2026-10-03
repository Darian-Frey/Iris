//! Packet building and parsing for the EVision V1 protocol spoken by the
//! Tecware Phantom RGB (`320f:5064`).
//!
//! This crate performs no I/O. It owns the command allow-list as a closed
//! enum: no API here accepts an arbitrary command byte (D-005, AV-001).
//! PROTOCOL.md is the specification; every constant in this crate should be
//! traceable to a tagged fact there.

#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
