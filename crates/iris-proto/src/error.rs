use std::fmt;

/// Errors from building or parsing packets. Parsing never panics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A colour write held more colours than fit in one packet.
    TooManyColours { count: usize, max: usize },
    /// A colour write had no colours.
    NoColours,
    /// An LED index lies outside the colour space.
    LedOutOfRange { led: usize },
    /// A received report was not 64 bytes.
    ReplyLength(usize),
    /// A received report carried a report ID other than `0x04` (a key press,
    /// media key or mouse report). Skip it and keep reading (AV-008).
    ForeignReport(u8),
    /// A vendor reply echoed a command byte outside the allow-list.
    UnknownReplyCommand(u8),
    /// A vendor reply declared a payload larger than a packet can hold.
    ReplySizeTooLarge(u8),
    /// A reply payload was shorter than the block it should contain.
    PayloadTooShort { len: usize, need: usize },
    /// The capability block did not begin with the V1 magic `55 aa`.
    BadMagic([u8; 2]),
    /// The LED map file was malformed.
    LedMap(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooManyColours { count, max } => {
                write!(
                    f,
                    "{count} colours do not fit in one packet (maximum {max})"
                )
            }
            Error::NoColours => write!(f, "colour write with no colours"),
            Error::LedOutOfRange { led } => write!(f, "LED index {led} is outside colour space"),
            Error::ReplyLength(len) => write!(f, "report is {len} bytes, expected 64"),
            Error::ForeignReport(id) => write!(f, "report ID {id:#04x} is not a vendor reply"),
            Error::UnknownReplyCommand(byte) => {
                write!(
                    f,
                    "reply echoes command {byte:#04x}, which is not allow-listed"
                )
            }
            Error::ReplySizeTooLarge(size) => write!(f, "reply declares payload size {size}"),
            Error::PayloadTooShort { len, need } => {
                write!(f, "payload is {len} bytes, need at least {need}")
            }
            Error::BadMagic(magic) => write!(
                f,
                "capability magic is {:02x} {:02x}, expected 55 aa",
                magic[0], magic[1]
            ),
            Error::LedMap(message) => write!(f, "LED map: {message}"),
        }
    }
}

impl std::error::Error for Error {}
