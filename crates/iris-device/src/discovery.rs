//! Finding the keyboard through `/sys/class/hidraw` (F-001, D-003).
//!
//! udev enumeration is unreliable inside the Flatpak sandbox, and hidraw
//! numbers change across replug and resume, so discovery reads sysfs every
//! time and never assumes a node name.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::DeviceError;

/// USB vendor ID of the reference keyboard. HW.
pub const VENDOR_ID: u16 = 0x320F;
/// USB product ID of the reference keyboard. HW.
pub const PRODUCT_ID: u16 = 0x5064;
/// Usage page of the vendor channel on interface 1. HW.
pub const VENDOR_USAGE_PAGE: u32 = 0xFF1C;

const SYS_CLASS_HIDRAW: &str = "/sys/class/hidraw";
const DEV: &str = "/dev";

/// What sysfs says about the USB device behind a hidraw node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbIdentity {
    pub vendor_id: u16,
    pub product_id: u16,
    /// Firmware revision (`bcdDevice`), if sysfs exposes it.
    pub bcd_device: Option<u16>,
}

/// A hidraw node carrying the keyboard's vendor channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The device node, for example `/dev/hidraw1`.
    pub node: PathBuf,
    pub usb: UsbIdentity,
    /// USB interface number, if sysfs exposes it. 1 on the reference board.
    pub interface: Option<u8>,
}

/// Finds every hidraw node that belongs to a `320f:5064` keyboard and whose
/// report descriptor declares the vendor usage page `0xFF1C`. The boot
/// keyboard interface never qualifies, so it is never opened.
pub fn discover() -> Result<Vec<Candidate>, DeviceError> {
    discover_in(Path::new(SYS_CLASS_HIDRAW), Path::new(DEV))
}

/// [`discover`] against an arbitrary sysfs class directory and device
/// directory, for tests.
pub fn discover_in(class_dir: &Path, dev_dir: &Path) -> Result<Vec<Candidate>, DeviceError> {
    let entries = match fs::read_dir(class_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(DeviceError::Io(error)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(DeviceError::Io)?;
        let name = entry.file_name();
        // A node can vanish mid-scan on unplug; skip anything unreadable.
        if let Some(candidate) = inspect(&entry.path(), &dev_dir.join(&name)) {
            found.push(candidate);
        }
    }
    found.sort_by(|a, b| a.node.cmp(&b.node));
    Ok(found)
}

fn inspect(class_entry: &Path, node: &Path) -> Option<Candidate> {
    let device = class_entry.join("device");
    let uevent = fs::read_to_string(device.join("uevent")).ok()?;
    let (vendor_id, product_id) = parse_hid_id(&uevent)?;
    if (vendor_id, product_id) != (VENDOR_ID, PRODUCT_ID) {
        return None;
    }
    let descriptor = fs::read(device.join("report_descriptor")).ok()?;
    if !usage_pages(&descriptor).contains(&VENDOR_USAGE_PAGE) {
        return None;
    }
    // device -> HID device; its parent is the USB interface, whose parent is
    // the USB device that carries bcdDevice.
    let hid = fs::canonicalize(&device).ok();
    let interface_dir = hid.as_deref().and_then(Path::parent);
    let usb_dir = interface_dir.and_then(Path::parent);
    let interface = interface_dir
        .and_then(|dir| read_hex(&dir.join("bInterfaceNumber")))
        .and_then(|n| u8::try_from(n).ok());
    let bcd_device = usb_dir.and_then(|dir| read_hex(&dir.join("bcdDevice")));
    Some(Candidate {
        node: node.to_path_buf(),
        usb: UsbIdentity {
            vendor_id,
            product_id,
            bcd_device,
        },
        interface,
    })
}

fn read_hex(path: &Path) -> Option<u16> {
    u16::from_str_radix(fs::read_to_string(path).ok()?.trim(), 16).ok()
}

/// Vendor and product ID from a uevent's `HID_ID=0003:0000320F:00005064`.
fn parse_hid_id(uevent: &str) -> Option<(u16, u16)> {
    let value = uevent
        .lines()
        .find_map(|line| line.strip_prefix("HID_ID="))?;
    let mut fields = value.trim().split(':');
    let _bus = fields.next()?;
    let vendor = u32::from_str_radix(fields.next()?, 16).ok()?;
    let product = u32::from_str_radix(fields.next()?, 16).ok()?;
    Some((u16::try_from(vendor).ok()?, u16::try_from(product).ok()?))
}

/// Every usage page declared by a HID report descriptor's Usage Page items.
/// Walks short and long items properly rather than searching for byte
/// patterns, which could match inside unrelated item data.
pub fn usage_pages(descriptor: &[u8]) -> Vec<u32> {
    let mut pages = Vec::new();
    let mut i = 0;
    while let Some(&prefix) = descriptor.get(i) {
        if prefix == 0xFE {
            // Long item: size byte, tag byte, then data.
            let Some(&size) = descriptor.get(i + 1) else {
                break;
            };
            i += 3 + usize::from(size);
            continue;
        }
        let size = [0, 1, 2, 4][usize::from(prefix & 0x03)];
        let Some(data) = descriptor.get(i + 1..i + 1 + size) else {
            break;
        };
        let item_type = (prefix >> 2) & 0x03;
        let tag = prefix >> 4;
        if item_type == 1 && tag == 0 {
            let value = data
                .iter()
                .rev()
                .fold(0u32, |acc, &byte| (acc << 8) | u32::from(byte));
            pages.push(value);
        }
        i += 1 + size;
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// The interface 1 descriptor from the 2026-10-04 capture.
    fn captured_descriptor() -> Vec<u8> {
        iris_proto::capture::REFERENCE_2026_10_04
            .lines()
            .skip_while(|line| !line.starts_with("Report descriptor"))
            .skip(1)
            .take_while(|line| !line.trim().is_empty())
            .filter_map(|line| line.trim().split_once(": "))
            .flat_map(|(_, hex)| hex.split_whitespace())
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect()
    }

    /// A minimal boot-keyboard descriptor, as on interface 0.
    const BOOT_KEYBOARD: [u8; 12] = [
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7,
    ];

    #[test]
    fn captured_descriptor_declares_the_vendor_page() {
        let descriptor = captured_descriptor();
        assert_eq!(descriptor.len(), 194);
        let pages = usage_pages(&descriptor);
        assert!(pages.contains(&VENDOR_USAGE_PAGE));
        // Generic desktop, keyboard, consumer, vendor, buttons.
        for page in [0x01, 0x07, 0x0C, 0x09] {
            assert!(pages.contains(&page), "{page:#x}");
        }
    }

    #[test]
    fn boot_keyboard_descriptor_does_not() {
        assert_eq!(usage_pages(&BOOT_KEYBOARD), [0x01, 0x07]);
    }

    #[test]
    fn truncated_and_long_items_are_handled() {
        assert_eq!(usage_pages(&[0x06, 0x1c]), Vec::<u32>::new());
        assert_eq!(
            usage_pages(&[0xFE, 0x02, 0x00, 0x06, 0x1c, 0x05, 0x0c]),
            [0x0C]
        );
    }

    #[test]
    fn hid_id_parsing() {
        let uevent =
            "DRIVER=hid-generic\nHID_ID=0003:0000320F:00005064\nHID_NAME=SONIX USB DEVICE\n";
        assert_eq!(parse_hid_id(uevent), Some((0x320F, 0x5064)));
        assert_eq!(
            parse_hid_id("HID_ID=0003:00000C45:0000652F"),
            Some((0x0C45, 0x652F))
        );
        assert_eq!(parse_hid_id("HID_ID=garbage"), None);
        assert_eq!(parse_hid_id("DRIVER=x"), None);
    }

    /// Builds a sysfs-like tree mirroring the reference machine.
    struct FakeSysfs {
        root: PathBuf,
    }

    impl FakeSysfs {
        fn new(name: &str) -> FakeSysfs {
            let root = std::env::temp_dir()
                .join(format!("iris-device-test-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("class")).unwrap();
            FakeSysfs { root }
        }

        fn class(&self) -> PathBuf {
            self.root.join("class")
        }

        fn add(&self, node: &str, interface: u8, hid_id: &str, descriptor: &[u8]) {
            let usb = self.root.join("devices/usb3/3-1");
            let iface = usb.join(format!("3-1:1.{interface}"));
            let hid = iface.join(format!("0003:320F:5064.000{interface}"));
            fs::create_dir_all(&hid).unwrap();
            fs::write(usb.join("bcdDevice"), "0102\n").unwrap();
            fs::write(iface.join("bInterfaceNumber"), format!("0{interface}\n")).unwrap();
            fs::write(hid.join("uevent"), format!("HID_ID={hid_id}\n")).unwrap();
            fs::write(hid.join("report_descriptor"), descriptor).unwrap();
            let entry = self.class().join(node);
            fs::create_dir_all(&entry).unwrap();
            symlink(&hid, entry.join("device")).unwrap();
        }
    }

    impl Drop for FakeSysfs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn finds_only_the_vendor_interface() {
        let sysfs = FakeSysfs::new("reference");
        sysfs.add("hidraw0", 0, "0003:0000320F:00005064", &BOOT_KEYBOARD);
        sysfs.add(
            "hidraw1",
            1,
            "0003:0000320F:00005064",
            &captured_descriptor(),
        );
        let found = discover_in(&sysfs.class(), Path::new("/dev")).unwrap();
        assert_eq!(
            found,
            [Candidate {
                node: PathBuf::from("/dev/hidraw1"),
                usb: UsbIdentity {
                    vendor_id: VENDOR_ID,
                    product_id: PRODUCT_ID,
                    bcd_device: Some(0x0102),
                },
                interface: Some(1),
            }]
        );
    }

    #[test]
    fn ignores_other_devices_and_renumbering_is_harmless() {
        let sysfs = FakeSysfs::new("renumbered");
        sysfs.add(
            "hidraw7",
            1,
            "0003:0000320F:00005064",
            &captured_descriptor(),
        );
        sysfs.add(
            "hidraw2",
            0,
            "0003:00000C45:0000652F",
            &captured_descriptor(),
        );
        let found = discover_in(&sysfs.class(), Path::new("/dev")).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node, PathBuf::from("/dev/hidraw7"));
    }

    #[test]
    fn missing_class_directory_means_nothing_found() {
        let found = discover_in(Path::new("/nonexistent/iris"), Path::new("/dev")).unwrap();
        assert!(found.is_empty());
    }
}
