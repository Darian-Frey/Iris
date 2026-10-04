//! Lists hidraw nodes carrying the keyboard's vendor channel. Reads sysfs
//! only; never opens a device node.

use std::process::ExitCode;

fn main() -> ExitCode {
    match iris_device::discover() {
        Ok(found) if found.is_empty() => {
            eprintln!("no 320f:5064 vendor interface found");
            ExitCode::FAILURE
        }
        Ok(found) => {
            for candidate in found {
                let bcd = candidate.usb.bcd_device.map_or("unknown".to_string(), |b| {
                    format!("{:x}.{:02x}", b >> 8, b & 0xFF)
                });
                let interface = candidate
                    .interface
                    .map_or("unknown".to_string(), |i| i.to_string());
                println!(
                    "{}  {:04x}:{:04x}  firmware {bcd}  interface {interface}",
                    candidate.node.display(),
                    candidate.usb.vendor_id,
                    candidate.usb.product_id,
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("discovery failed: {error}");
            ExitCode::FAILURE
        }
    }
}
