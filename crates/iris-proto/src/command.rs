//! The command allow-list (PROTOCOL.md §3, D-005).

/// Every command Iris is permitted to send.
///
/// This enum *is* the allow-list. A variant may be added only when the
/// command is tagged HW in PROTOCOL.md or an explicit gated-experiment note
/// authorises it (D-005), and the allow-list test must be updated in the same
/// change. Commands `0x04`, `0x08` and `0x0A` are deliberately absent
/// (PROTOCOL.md §9, D-015); EVision V2 commands are never added (D-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Command {
    /// Begin a write transaction. SRC.
    Begin = 0x01,
    /// End a write transaction. SRC.
    End = 0x02,
    /// Read the capability block. HW.
    ReadCapabilities = 0x03,
    /// Read configuration space. HW.
    ReadConfig = 0x05,
    /// Set one parameter in configuration space. SRC, path validated via OpenRGB.
    SetParameter = 0x06,
    /// Read custom colour space. HW.
    ReadColours = 0x10,
    /// Write custom colour space. SRC, validated via OpenRGB.
    WriteColours = 0x11,
}

impl Command {
    /// Every variant, in byte order.
    pub const ALL: [Command; 7] = [
        Command::Begin,
        Command::End,
        Command::ReadCapabilities,
        Command::ReadConfig,
        Command::SetParameter,
        Command::ReadColours,
        Command::WriteColours,
    ];

    /// The command byte placed at packet offset 3.
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// Whether the command changes device state.
    pub const fn is_write(self) -> bool {
        matches!(
            self,
            Command::Begin | Command::End | Command::SetParameter | Command::WriteColours
        )
    }

    /// Recognises an allow-listed command byte, for parsing replies.
    ///
    /// Returns `None` for every byte outside the allow-list, so it cannot be
    /// used to smuggle an arbitrary command into a request.
    pub const fn from_byte(byte: u8) -> Option<Command> {
        match byte {
            0x01 => Some(Command::Begin),
            0x02 => Some(Command::End),
            0x03 => Some(Command::ReadCapabilities),
            0x05 => Some(Command::ReadConfig),
            0x06 => Some(Command::SetParameter),
            0x10 => Some(Command::ReadColours),
            0x11 => Some(Command::WriteColours),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The allow-list from PROTOCOL.md §Command allow-list, copied by hand.
    /// If this test fails, the enum and the specification have diverged.
    const PROTOCOL_ALLOW_LIST: [u8; 7] = [0x01, 0x02, 0x03, 0x05, 0x06, 0x10, 0x11];

    #[test]
    fn allow_list_matches_protocol() {
        let bytes: Vec<u8> = Command::ALL.iter().map(|c| c.byte()).collect();
        assert_eq!(bytes, PROTOCOL_ALLOW_LIST);
    }

    #[test]
    fn from_byte_accepts_only_the_allow_list() {
        for byte in 0..=u8::MAX {
            let parsed = Command::from_byte(byte);
            assert_eq!(
                parsed.is_some(),
                PROTOCOL_ALLOW_LIST.contains(&byte),
                "{byte:#04x}"
            );
            if let Some(command) = parsed {
                assert_eq!(command.byte(), byte);
            }
        }
    }

    #[test]
    fn forbidden_commands_are_absent() {
        // 0x04 capability write, 0x08/0x0A keymap writes (D-015), 0x12/0x13 V2 direct mode (D-004).
        for byte in [0x04, 0x08, 0x0A, 0x12, 0x13] {
            assert!(Command::from_byte(byte).is_none(), "{byte:#04x}");
        }
    }
}
