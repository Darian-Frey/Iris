//! The 64-byte packet format (PROTOCOL.md §2).

use crate::address::{COLOUR_SLOTS, Profile, Rgb, colour_address, config_address};
use crate::capability::CAPABILITY_SIZE;
use crate::config::Setting;
use crate::{Command, Error, Result};

/// Every packet, request or reply, is 64 bytes. HW.
pub const PACKET_LEN: usize = 64;

/// Report ID of the vendor channel. HW.
pub const REPORT_ID: u8 = 0x04;

/// Bytes before the payload: report ID, checksum, command, size, offset, status.
pub const HEADER_LEN: usize = 8;

/// Largest payload a packet can carry (`0x38`). SRC.
pub const MAX_PAYLOAD: usize = PACKET_LEN - HEADER_LEN;

/// Largest colour payload: a whole number of RGB triples (`0x36`). SRC.
pub const MAX_COLOUR_PAYLOAD: usize = 54;

/// Read size used for a configuration block. This is the size the probe used
/// on hardware; it spans one profile's 42 bytes and part of the next. HW.
pub const READ_CONFIG_SIZE: u8 = 0x38;

const MAX_COLOURS_PER_PACKET: usize = MAX_COLOUR_PAYLOAD / 3;

/// Whole 3-byte slots in one profile's `0x200`-byte colour region (170).
/// Only slots below [`COLOUR_SLOTS`] are in normal use; the rest are reached
/// solely by the `experimental_` constructors.
pub const COLOUR_REGION_SLOTS: usize = crate::COLOUR_STRIDE as usize / 3;

/// Checksum: 16-bit wrapping sum of bytes 3–63. HW.
pub fn checksum(packet: &[u8; PACKET_LEN]) -> u16 {
    packet[3..]
        .iter()
        .fold(0u16, |sum, &byte| sum.wrapping_add(u16::from(byte)))
}

/// A request to the keyboard.
///
/// Fields are private and every constructor is typed, so a request can only
/// carry an allow-listed command addressed at a documented location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    command: Command,
    size: u8,
    offset: u16,
    payload: [u8; MAX_PAYLOAD],
}

impl Request {
    fn new(command: Command, size: u8, offset: u16) -> Request {
        Request {
            command,
            size,
            offset,
            payload: [0; MAX_PAYLOAD],
        }
    }

    /// Begin a write transaction (`04 01 00 01`, then zeros).
    pub fn begin() -> Request {
        Request::new(Command::Begin, 0, 0)
    }

    /// End a write transaction (`04 02 00 02`, then zeros).
    pub fn end() -> Request {
        Request::new(Command::End, 0, 0)
    }

    /// Read the 44-byte capability block.
    pub fn read_capabilities() -> Request {
        Request::new(Command::ReadCapabilities, CAPABILITY_SIZE as u8, 0)
    }

    /// Read one profile's configuration block.
    pub fn read_config(profile: Profile) -> Request {
        Request::new(
            Command::ReadConfig,
            READ_CONFIG_SIZE,
            u16::from(profile.index()) * crate::CONFIG_STRIDE,
        )
    }

    /// Set one parameter of one profile. Must be sent inside a transaction.
    pub fn set_parameter(profile: Profile, setting: Setting) -> Request {
        let parameter = setting.parameter();
        let (bytes, len) = setting.encode();
        let mut request = Request::new(
            Command::SetParameter,
            len as u8,
            config_address(profile, parameter),
        );
        request.payload[..len].copy_from_slice(&bytes[..len]);
        request
    }

    /// Read `count` consecutive LED colours (at most 18) starting at `first_led`.
    pub fn read_colours(profile: Profile, first_led: usize, count: usize) -> Result<Request> {
        let offset = colour_run_offset(profile, first_led, count)?;
        Ok(Request::new(
            Command::ReadColours,
            (count * 3) as u8,
            offset,
        ))
    }

    /// Requests that read a profile's whole colour map, 18 LEDs per packet.
    pub fn read_colour_map(profile: Profile) -> Vec<Request> {
        (0..COLOUR_SLOTS)
            .step_by(MAX_COLOURS_PER_PACKET)
            .map(|first| {
                let count = MAX_COLOURS_PER_PACKET.min(COLOUR_SLOTS - first);
                // In range by construction: first + count <= COLOUR_SLOTS.
                let offset = u16::from(profile.index()) * crate::COLOUR_STRIDE + first as u16 * 3;
                Request::new(Command::ReadColours, (count * 3) as u8, offset)
            })
            .collect()
    }

    /// Write consecutive LED colours (at most 18) starting at `first_led`.
    /// Must be sent inside a transaction.
    pub fn write_colours(profile: Profile, first_led: usize, colours: &[Rgb]) -> Result<Request> {
        let offset = colour_run_offset(profile, first_led, colours.len())?;
        let mut request = Request::new(Command::WriteColours, (colours.len() * 3) as u8, offset);
        for (slot, colour) in request.payload.chunks_exact_mut(3).zip(colours) {
            slot.copy_from_slice(&[colour.r, colour.g, colour.b]);
        }
        Ok(request)
    }

    /// Write any number of consecutive LED colours, split into packets of at
    /// most 18 colours. Must be sent inside one transaction.
    pub fn write_colour_run(
        profile: Profile,
        first_led: usize,
        colours: &[Rgb],
    ) -> Result<Vec<Request>> {
        if colours.is_empty() {
            return Err(Error::NoColours);
        }
        let last = first_led
            .checked_add(colours.len() - 1)
            .ok_or(Error::LedOutOfRange { led: usize::MAX })?;
        colour_address(profile, last)?;
        colours
            .chunks(MAX_COLOURS_PER_PACKET)
            .enumerate()
            .map(|(i, chunk)| {
                Request::write_colours(profile, first_led + i * MAX_COLOURS_PER_PACKET, chunk)
            })
            .collect()
    }

    /// EXPERIMENTAL. Reads up to 18 slots anywhere in a profile's whole
    /// `0x200`-byte colour region, including slots 118–169, which lie beyond
    /// the 118 the LED map knows and whose meaning is OPEN (PROTOCOL.md §9
    /// item 8). For the gated slot probe only.
    pub fn experimental_read_region_colours(
        profile: Profile,
        first_slot: usize,
        count: usize,
    ) -> Result<Request> {
        let offset = colour_run_offset_within(profile, first_slot, count, COLOUR_REGION_SLOTS)?;
        Ok(Request::new(
            Command::ReadColours,
            (count * 3) as u8,
            offset,
        ))
    }

    /// EXPERIMENTAL. Writes up to 18 slots anywhere in a profile's colour
    /// region (see [`experimental_read_region_colours`](Self::experimental_read_region_colours)).
    /// Callers must read the slots first and write the original bytes back
    /// (AV-016). Must be sent inside a transaction.
    pub fn experimental_write_region_colours(
        profile: Profile,
        first_slot: usize,
        colours: &[Rgb],
    ) -> Result<Request> {
        let offset =
            colour_run_offset_within(profile, first_slot, colours.len(), COLOUR_REGION_SLOTS)?;
        let mut request = Request::new(Command::WriteColours, (colours.len() * 3) as u8, offset);
        for (slot, colour) in request.payload.chunks_exact_mut(3).zip(colours) {
            slot.copy_from_slice(&[colour.r, colour.g, colour.b]);
        }
        Ok(request)
    }

    pub fn command(&self) -> Command {
        self.command
    }

    pub fn size(&self) -> u8 {
        self.size
    }

    pub fn offset(&self) -> u16 {
        self.offset
    }

    /// The payload bytes, `size` long.
    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.size)]
    }

    /// The 64 bytes to write to the hidraw node.
    pub fn encode(&self) -> [u8; PACKET_LEN] {
        let mut packet = [0u8; PACKET_LEN];
        packet[0] = REPORT_ID;
        packet[3] = self.command.byte();
        packet[4] = self.size;
        packet[5..7].copy_from_slice(&self.offset.to_le_bytes());
        packet[HEADER_LEN..].copy_from_slice(&self.payload);
        let sum = checksum(&packet);
        packet[1..3].copy_from_slice(&sum.to_le_bytes());
        packet
    }
}

/// Validates a run of LED colours and returns the address of its first LED.
fn colour_run_offset(profile: Profile, first_led: usize, count: usize) -> Result<u16> {
    colour_run_offset_within(profile, first_led, count, COLOUR_SLOTS)
}

/// Validates a run of `count` slots starting at `first`, all below `slots`,
/// and returns the address of the first.
fn colour_run_offset_within(
    profile: Profile,
    first: usize,
    count: usize,
    slots: usize,
) -> Result<u16> {
    if count == 0 {
        return Err(Error::NoColours);
    }
    if count > MAX_COLOURS_PER_PACKET {
        return Err(Error::TooManyColours {
            count,
            max: MAX_COLOURS_PER_PACKET,
        });
    }
    let last = first
        .checked_add(count - 1)
        .ok_or(Error::LedOutOfRange { led: usize::MAX })?;
    if last >= slots {
        return Err(Error::LedOutOfRange { led: last });
    }
    // last < slots <= COLOUR_REGION_SLOTS, so the offset fits in the profile's region.
    Ok(u16::from(profile.index()) * crate::COLOUR_STRIDE + first as u16 * 3)
}

/// A vendor reply from the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    command: Command,
    bytes: [u8; PACKET_LEN],
}

impl Reply {
    /// Parses one report read from the hidraw node.
    ///
    /// Reports with another report ID are key presses or media keys sharing
    /// the node; they yield [`Error::ForeignReport`] and must be skipped, not
    /// treated as the reply (AV-008).
    pub fn parse(report: &[u8]) -> Result<Reply> {
        let bytes: [u8; PACKET_LEN] = report.try_into().map_err(|_| match report.first() {
            Some(&id) if id != REPORT_ID => Error::ForeignReport(id),
            _ => Error::ReplyLength(report.len()),
        })?;
        if bytes[0] != REPORT_ID {
            return Err(Error::ForeignReport(bytes[0]));
        }
        let command = Command::from_byte(bytes[3]).ok_or(Error::UnknownReplyCommand(bytes[3]))?;
        if usize::from(bytes[4]) > MAX_PAYLOAD {
            return Err(Error::ReplySizeTooLarge(bytes[4]));
        }
        Ok(Reply { command, bytes })
    }

    /// Whether this reply answers `request`: bytes 1–6 (checksum, command,
    /// size, offset) must echo the request (ARCHITECTURE.md invariant 9).
    pub fn answers(&self, request: &Request) -> bool {
        self.bytes[1..7] == request.encode()[1..7]
    }

    pub fn command(&self) -> Command {
        self.command
    }

    pub fn size(&self) -> u8 {
        self.bytes[4]
    }

    pub fn offset(&self) -> u16 {
        u16::from_le_bytes([self.bytes[5], self.bytes[6]])
    }

    /// Byte 7. Always `0x00` on this firmware so far, even for a misused
    /// command; it proves nothing about whether a request was understood
    /// (PROTOCOL.md §2, OPEN).
    pub fn status(&self) -> u8 {
        self.bytes[7]
    }

    /// The payload bytes, `size` long.
    pub fn payload(&self) -> &[u8] {
        &self.bytes[HEADER_LEN..HEADER_LEN + usize::from(self.size())]
    }

    /// The raw 64 bytes.
    pub fn as_bytes(&self) -> &[u8; PACKET_LEN] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mode;

    fn reply_to(request: &Request, payload: &[u8]) -> [u8; PACKET_LEN] {
        let mut bytes = request.encode();
        bytes[HEADER_LEN..].fill(0);
        bytes[HEADER_LEN..HEADER_LEN + payload.len()].copy_from_slice(payload);
        bytes
    }

    #[test]
    fn read_capabilities_matches_known_good_packet() {
        // PROTOCOL.md §4 / CLAUDE.md: 04 2f 00 03 2c, then zeros.
        let packet = Request::read_capabilities().encode();
        assert_eq!(packet[..7], [0x04, 0x2f, 0x00, 0x03, 0x2c, 0x00, 0x00]);
        assert!(packet[7..].iter().all(|&b| b == 0));
    }

    #[test]
    fn read_config_profile_two_matches_known_good_packet() {
        // 04 67 00 05 38 2a, then zeros.
        let packet = Request::read_config(Profile::Two).encode();
        assert_eq!(packet[..7], [0x04, 0x67, 0x00, 0x05, 0x38, 0x2a, 0x00]);
        assert!(packet[7..].iter().all(|&b| b == 0));
    }

    #[test]
    fn begin_and_end_match_protocol() {
        assert_eq!(
            Request::begin().encode()[..5],
            [0x04, 0x01, 0x00, 0x01, 0x00]
        );
        assert_eq!(Request::end().encode()[..5], [0x04, 0x02, 0x00, 0x02, 0x00]);
    }

    #[test]
    fn checksum_covers_bytes_3_to_63_only() {
        let mut packet = [0u8; PACKET_LEN];
        packet[..3].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
        assert_eq!(checksum(&packet), 0);
        packet[3..].fill(0xFF);
        // 61 × 0xFF: the largest possible sum, which fits in 16 bits.
        assert_eq!(checksum(&packet), 61 * 0xFF);
    }

    #[test]
    fn every_constructible_request_is_allow_listed() {
        let mut requests = vec![
            Request::begin(),
            Request::end(),
            Request::read_capabilities(),
            Request::set_parameter(Profile::One, Setting::Mode(Mode::Static)),
            Request::set_parameter(Profile::Three, Setting::ModeColour(Rgb::new(1, 2, 3))),
        ];
        requests.extend(Profile::ALL.map(Request::read_config));
        requests.extend(Request::read_colour_map(Profile::Two));
        requests.extend(
            Request::write_colour_run(Profile::One, 0, &[Rgb::default(); COLOUR_SLOTS]).unwrap(),
        );
        for request in requests {
            let byte = request.encode()[3];
            assert!(Command::from_byte(byte).is_some(), "{byte:#04x}");
        }
    }

    #[test]
    fn set_parameter_layout() {
        let packet = Request::set_parameter(
            Profile::Two,
            Setting::ModeColour(Rgb::new(0x96, 0x96, 0x9a)),
        )
        .encode();
        assert_eq!(packet[3..10], [0x06, 0x03, 0x2f, 0x00, 0x00, 0x96, 0x96]);
        assert_eq!(packet[10], 0x9a);
        assert!(packet[11..].iter().all(|&b| b == 0));
    }

    #[test]
    fn single_key_colour_write() {
        // LED 59 = J: one 3-byte packet at offset 177.
        let request = Request::write_colours(Profile::One, 59, &[Rgb::new(255, 0, 0)]).unwrap();
        let packet = request.encode();
        assert_eq!(packet[3..11], [0x11, 0x03, 177, 0x00, 0x00, 255, 0, 0]);
    }

    #[test]
    fn full_map_is_seven_packets_of_at_most_54_bytes() {
        let colours = [Rgb::new(1, 2, 3); COLOUR_SLOTS];
        let requests = Request::write_colour_run(Profile::Two, 0, &colours).unwrap();
        assert_eq!(requests.len(), 7);
        let total: usize = requests.iter().map(|r| usize::from(r.size())).sum();
        assert_eq!(total, 354);
        for (i, request) in requests.iter().enumerate() {
            assert!(usize::from(request.size()) <= MAX_COLOUR_PAYLOAD);
            assert_eq!(request.offset(), 0x200 + (i * 54) as u16);
        }
        assert_eq!(Request::read_colour_map(Profile::Two).len(), 7);
    }

    #[test]
    fn colour_runs_are_bounded() {
        let colours = [Rgb::default(); 19];
        assert_eq!(
            Request::write_colours(Profile::One, 0, &colours),
            Err(Error::TooManyColours { count: 19, max: 18 })
        );
        assert_eq!(
            Request::write_colours(Profile::One, 0, &[]),
            Err(Error::NoColours)
        );
        assert_eq!(
            Request::write_colours(Profile::One, 117, &[Rgb::default(); 2]),
            Err(Error::LedOutOfRange { led: 118 })
        );
        assert!(Request::write_colour_run(Profile::One, 100, &[Rgb::default(); 19]).is_err());
        assert!(Request::write_colour_run(Profile::One, 0, &[]).is_err());
    }

    #[test]
    fn experimental_region_reaches_slot_169_and_no_further() {
        let request =
            Request::experimental_write_region_colours(Profile::One, 169, &[Rgb::new(1, 2, 3)])
                .unwrap();
        assert_eq!(request.offset(), 507);
        assert_eq!(request.encode()[3], Command::WriteColours.byte());
        let request = Request::experimental_read_region_colours(Profile::Two, 118, 18).unwrap();
        assert_eq!(request.offset(), 0x200 + 354);
        assert_eq!(
            Request::experimental_write_region_colours(Profile::One, 170, &[Rgb::default()]),
            Err(Error::LedOutOfRange { led: 170 })
        );
        assert_eq!(
            Request::experimental_read_region_colours(Profile::One, 160, 11),
            Err(Error::LedOutOfRange { led: 170 })
        );
        // The normal constructors still stop at 117.
        assert!(Request::write_colours(Profile::One, 118, &[Rgb::default()]).is_err());
    }

    #[test]
    fn reply_parses_and_matches_its_request() {
        let request = Request::read_config(Profile::Two);
        let reply = Reply::parse(&reply_to(&request, &[0x01, 0x04, 0x02])).unwrap();
        assert!(reply.answers(&request));
        assert!(!reply.answers(&Request::read_config(Profile::Three)));
        assert_eq!(reply.command(), Command::ReadConfig);
        assert_eq!(reply.offset(), 0x2A);
        assert_eq!(reply.payload().len(), 0x38);
        assert_eq!(reply.payload()[..3], [0x01, 0x04, 0x02]);
    }

    #[test]
    fn foreign_reports_are_rejected() {
        // Report ID 1: NKRO key bitmap arriving between request and reply (AV-008).
        let mut key_press = [0u8; PACKET_LEN];
        key_press[0] = 0x01;
        assert_eq!(Reply::parse(&key_press), Err(Error::ForeignReport(0x01)));
        // Short consumer-control report.
        assert_eq!(
            Reply::parse(&[0x03, 0xe9, 0x00]),
            Err(Error::ForeignReport(0x03))
        );
        assert_eq!(Reply::parse(&[]), Err(Error::ReplyLength(0)));
        assert_eq!(Reply::parse(&[0x04; 10]), Err(Error::ReplyLength(10)));
    }

    #[test]
    fn malformed_vendor_replies_are_rejected() {
        let mut bytes = Request::read_capabilities().encode();
        bytes[3] = 0x0A;
        assert_eq!(Reply::parse(&bytes), Err(Error::UnknownReplyCommand(0x0A)));
        let mut bytes = Request::read_capabilities().encode();
        bytes[4] = 57;
        assert_eq!(Reply::parse(&bytes), Err(Error::ReplySizeTooLarge(57)));
    }

    #[test]
    fn interleaved_reports_leave_the_reply_intact() {
        let request = Request::read_capabilities();
        let stream: Vec<Vec<u8>> = vec![
            {
                let mut r = vec![0u8; PACKET_LEN];
                r[0] = 0x01;
                r
            },
            vec![0x03, 0xe9, 0x00],
            reply_to(&request, &[0x55, 0xaa]).to_vec(),
        ];
        let reply = stream
            .iter()
            .filter_map(|report| Reply::parse(report).ok())
            .find(|reply| reply.answers(&request))
            .unwrap();
        assert_eq!(reply.payload()[..2], [0x55, 0xaa]);
    }

    #[test]
    fn captured_replies_answer_their_requests() {
        use crate::fixtures::{CAPABILITIES, COLOURS, CONFIG, probe_reply};
        let mut pairs = vec![(CAPABILITIES, Request::read_capabilities())];
        for (label, profile) in CONFIG.into_iter().zip(Profile::ALL) {
            pairs.push((label, Request::read_config(profile)));
        }
        pairs.push((COLOURS, Request::read_colour_map(Profile::One)[0].clone()));
        for (label, request) in pairs {
            let reply = Reply::parse(&probe_reply(label)).unwrap();
            assert!(reply.answers(&request), "{label}");
            assert_eq!(reply.as_bytes()[1..7], request.encode()[1..7], "{label}");
            assert_eq!(reply.status(), 0, "{label}");
        }
    }

    #[test]
    fn captured_colour_reply_holds_eighteen_leds() {
        let reply = Reply::parse(&crate::fixtures::probe_reply(crate::fixtures::COLOURS)).unwrap();
        assert_eq!(reply.payload().len(), MAX_COLOUR_PAYLOAD);
        // LEDs 0-17 of profile 1 read back red on 2026-10-04.
        assert!(
            reply
                .payload()
                .chunks_exact(3)
                .all(|rgb| rgb == [0xff, 0x00, 0x00])
        );
    }
}
