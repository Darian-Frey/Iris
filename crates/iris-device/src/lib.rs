//! Talking to the keyboard: discovery through sysfs, the hidraw transport,
//! the identity check, matched request/reply exchanges and write
//! transactions that always close. Behind the `sim` feature, an in-memory
//! keyboard that speaks EVision V1 for tests.
//!
//! Only `irisd` uses this crate to open the device (D-003). Everything sent
//! goes through [`Transport::send`], which accepts an [`iris_proto::Request`]
//! and nothing else, so the allow-list holds at this layer too (D-005).

#![deny(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod device;
mod discovery;
mod error;
mod hidraw;
mod transport;

pub use device::{DEFAULT_TIMEOUT, Device, REFERENCE_BCD_DEVICE, Transaction, WriteAccess};
pub use discovery::{
    Candidate, PRODUCT_ID, UsbIdentity, VENDOR_ID, VENDOR_USAGE_PAGE, discover, discover_in,
    usage_pages,
};
pub use error::DeviceError;
pub use hidraw::Hidraw;
pub use transport::{Transport, TransportError};

#[cfg(any(test, feature = "sim"))]
pub mod sim;

/// The host udev rule that grants the logged-in user access (F-025).
pub const UDEV_RULE: &str = include_str!("../../../data/60-iris-keyboard.rules");

/// Instructions for fixing [`DeviceError::PermissionDenied`], with the exact
/// rule and commands (F-025, AV-010). A Flatpak cannot install udev rules,
/// so the user must run these on the host once.
pub fn permission_help() -> String {
    format!(
        "Iris cannot open the keyboard. The host needs a udev rule, which a \
         Flatpak cannot install for you.\n\n\
         Save this as /etc/udev/rules.d/60-iris-keyboard.rules:\n\n{UDEV_RULE}\n\
         Then run:\n\n\
         \x20   sudo udevadm control --reload-rules\n\
         \x20   sudo udevadm trigger\n\n\
         and unplug and replug the keyboard.\n"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn permission_help_names_the_device() {
        let help = super::permission_help();
        assert!(help.contains(r#"ATTRS{idVendor}=="320f""#));
        assert!(help.contains(r#"ATTRS{idProduct}=="5064""#));
        assert!(help.contains("udevadm trigger"));
    }
}
