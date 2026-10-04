//! Talking to the keyboard: the transport seam, and (behind the `sim`
//! feature) an in-memory keyboard that speaks EVision V1 for tests.
//!
//! Only `irisd` uses this crate to open the device (D-003). Everything sent
//! goes through [`Transport::send`], which accepts an [`iris_proto::Request`]
//! and nothing else, so the allow-list holds at this layer too (D-005).

#![deny(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod transport;

pub use transport::{Transport, TransportError};

#[cfg(any(test, feature = "sim"))]
pub mod sim;
