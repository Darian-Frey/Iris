//! An in-memory keyboard that speaks EVision V1 (ARCHITECTURE.md §Testing
//! seam).
//!
//! It decodes the 64 bytes of each request exactly as the device would see
//! them, keeps capability, configuration and colour memory, and answers with
//! replies laid out like the real ones. Its starting state is the author's
//! probe capture of 2026-10-04, and its replies to the same five reads are
//! checked byte for byte against that capture.
//!
//! Where the real firmware's behaviour is unknown, the simulator takes the
//! strictest reasonable model and records anything a well-behaved client
//! should never do as a [`Violation`], so tests can assert there are none.
//! These are modelling choices, not protocol facts:
//!
//! - Writes (`0x06`, `0x11`) are staged inside a begin/end pair and applied
//!   only at end. A write outside a transaction is ignored. An abandoned
//!   transaction applies nothing (AV-005).
//! - Reads see committed state only.
//! - Memory the probe never read (most of the colour map, configuration
//!   space beyond the three profiles) starts as zero.
//! - Every request, including begin, end and writes, gets one reply that
//!   echoes bytes 1–6 with status `0x00` and, for non-reads, a zero payload.
//!   The one-reply rule and the zero status are HW; the write payload is not
//!   recorded anywhere.

use std::collections::VecDeque;
use std::time::Duration;

use iris_proto::{
    CAPABILITY_SIZE, COLOUR_SLOTS, COLOUR_STRIDE, CONFIG_STRIDE, Command, HEADER_LEN, MAX_PAYLOAD,
    PACKET_LEN, Profile, REPORT_ID, Request, Rgb, capture, checksum,
};

use crate::{Transport, TransportError};

/// Configuration bytes belonging to the three profiles.
const CONFIG_PROFILES_LEN: usize = 3 * CONFIG_STRIDE as usize;
/// Readable configuration space. Reads past the third profile return zeros
/// on hardware (a `0x38` read at `0x54` ends at `0x8C`).
const CONFIG_SPACE: usize = 0x100;
const COLOUR_SPACE: usize = 3 * COLOUR_STRIDE as usize;

/// Something a correct client should never do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// A report whose first byte is not the vendor report ID.
    BadReportId(u8),
    /// A checksum that does not match bytes 3–63. Whether the firmware checks
    /// it is OPEN; the request is still processed.
    BadChecksum { expected: u16, found: u16 },
    /// A command byte outside the allow-list. Answered like any other
    /// request, as the real board answered BUG-001's `0x0A`.
    UnknownCommand(u8),
    /// A declared payload size larger than a packet can hold.
    OversizePayload(u8),
    /// A data write with no transaction open. Ignored.
    WriteOutsideTransaction(Command),
    /// Begin while a transaction is already open.
    NestedBegin,
    /// End with no transaction open.
    EndWithoutBegin,
    /// A read or write beyond the memory the command addresses.
    OutOfRange {
        command: Command,
        offset: u16,
        size: u8,
    },
}

#[derive(Debug, Clone)]
struct StagedWrite {
    command: Command,
    offset: usize,
    data: Vec<u8>,
}

/// The simulated keyboard. Implements [`Transport`].
#[derive(Debug, Clone)]
pub struct SimulatedKeyboard {
    capabilities: [u8; CAPABILITY_SIZE],
    config: [u8; CONFIG_SPACE],
    colours: [u8; COLOUR_SPACE],
    transaction: Option<Vec<StagedWrite>>,
    outbox: VecDeque<Vec<u8>>,
    foreign: Vec<Vec<u8>>,
    replies_to_drop: usize,
    plugged_in: bool,
    received: Vec<[u8; PACKET_LEN]>,
    committed: usize,
    violations: Vec<Violation>,
}

impl SimulatedKeyboard {
    /// The reference board as captured on 2026-10-04: its capability block,
    /// its three configuration blocks and the first 18 LEDs of profile 1's
    /// colour map. Everything else starts as zero.
    pub fn reference() -> SimulatedKeyboard {
        let text = capture::REFERENCE_2026_10_04;
        let payload = |label: &str| -> Vec<u8> {
            // The capture is compiled in and checked by iris-proto's tests; a
            // missing reply leaves the corresponding memory zeroed.
            capture::probe_reply(text, label)
                .map(|reply| {
                    let size = usize::from(reply[4]).min(MAX_PAYLOAD);
                    reply[HEADER_LEN..HEADER_LEN + size].to_vec()
                })
                .unwrap_or_default()
        };

        let mut keyboard = SimulatedKeyboard {
            capabilities: [0; CAPABILITY_SIZE],
            config: [0; CONFIG_SPACE],
            colours: [0; COLOUR_SPACE],
            transaction: None,
            outbox: VecDeque::new(),
            foreign: Vec::new(),
            replies_to_drop: 0,
            plugged_in: true,
            received: Vec::new(),
            committed: 0,
            violations: Vec::new(),
        };
        copy_prefix(&mut keyboard.capabilities, &payload(capture::CAPABILITIES));
        for (profile, label) in capture::CONFIG.into_iter().enumerate() {
            let block = payload(label);
            let base = profile * usize::from(CONFIG_STRIDE);
            let len = block.len().min(usize::from(CONFIG_STRIDE));
            keyboard.config[base..base + len].copy_from_slice(&block[..len]);
        }
        copy_prefix(&mut keyboard.colours, &payload(capture::COLOURS));
        keyboard
    }

    /// The capability block as currently held.
    pub fn capabilities(&self) -> &[u8; CAPABILITY_SIZE] {
        &self.capabilities
    }

    /// One profile's committed 42-byte configuration block.
    pub fn config(&self, profile: Profile) -> &[u8] {
        let base = usize::from(profile.index()) * usize::from(CONFIG_STRIDE);
        &self.config[base..base + usize::from(CONFIG_STRIDE)]
    }

    /// One LED's committed colour, or `None` if `led` is outside colour space.
    pub fn colour(&self, profile: Profile, led: usize) -> Option<Rgb> {
        if led >= COLOUR_SLOTS {
            return None;
        }
        let at = usize::from(profile.index()) * usize::from(COLOUR_STRIDE) + led * 3;
        Some(Rgb::new(
            self.colours[at],
            self.colours[at + 1],
            self.colours[at + 2],
        ))
    }

    /// Every packet received, in order.
    pub fn received(&self) -> &[[u8; PACKET_LEN]] {
        &self.received
    }

    /// Number of data-write packets (`0x06`, `0x11`) received, applied or
    /// not. Applying an unchanged profile must leave this unchanged (F-012).
    pub fn write_packets(&self) -> usize {
        self.received
            .iter()
            .filter(|p| {
                Command::from_byte(p[3])
                    .is_some_and(|c| matches!(c, Command::SetParameter | Command::WriteColours))
            })
            .count()
    }

    /// Number of transactions closed by an end packet.
    pub fn committed_transactions(&self) -> usize {
        self.committed
    }

    /// Whether a begin is waiting for its end.
    pub fn in_transaction(&self) -> bool {
        self.transaction.is_some()
    }

    /// Protocol misuse observed so far.
    pub fn violations(&self) -> &[Violation] {
        &self.violations
    }

    /// Queues a non-vendor input report (a key press, report ID 1, or a
    /// media key, report ID 3) to arrive just before the next reply (AV-008).
    pub fn queue_foreign_report(&mut self, report: Vec<u8>) {
        self.foreign.push(report);
    }

    /// Swallows the replies to the next `count` requests, so the client
    /// times out.
    pub fn drop_replies(&mut self, count: usize) {
        self.replies_to_drop += count;
    }

    /// Simulates unplugging: every send and receive fails until
    /// [`plug_in`](Self::plug_in). Undelivered reports and any open
    /// transaction are lost.
    pub fn unplug(&mut self) {
        self.plugged_in = false;
        self.outbox.clear();
        self.foreign.clear();
        self.transaction = None;
    }

    /// Reconnects with committed memory intact.
    pub fn plug_in(&mut self) {
        self.plugged_in = true;
    }

    fn handle(&mut self, packet: [u8; PACKET_LEN]) {
        self.received.push(packet);
        if packet[0] != REPORT_ID {
            self.violations.push(Violation::BadReportId(packet[0]));
            return;
        }
        let found = u16::from_le_bytes([packet[1], packet[2]]);
        let expected = checksum(&packet);
        if found != expected {
            self.violations
                .push(Violation::BadChecksum { expected, found });
        }

        let mut reply = [0u8; PACKET_LEN];
        reply[..7].copy_from_slice(&packet[..7]);

        let size = packet[4];
        if usize::from(size) > MAX_PAYLOAD {
            self.violations.push(Violation::OversizePayload(size));
        } else if let Some(command) = Command::from_byte(packet[3]) {
            let offset = u16::from_le_bytes([packet[5], packet[6]]);
            let data = &packet[HEADER_LEN..HEADER_LEN + usize::from(size)];
            if let Some(read) = self.execute(command, offset, size, data) {
                reply[HEADER_LEN..HEADER_LEN + read.len()].copy_from_slice(&read);
            }
        } else {
            self.violations.push(Violation::UnknownCommand(packet[3]));
        }

        self.outbox.extend(self.foreign.drain(..));
        if self.replies_to_drop > 0 {
            self.replies_to_drop -= 1;
        } else {
            self.outbox.push_back(reply.to_vec());
        }
    }

    /// Carries out one well-formed request; returns read data, if any.
    fn execute(&mut self, command: Command, offset: u16, size: u8, data: &[u8]) -> Option<Vec<u8>> {
        let start = usize::from(offset);
        let end = start + usize::from(size);
        let out_of_range = Violation::OutOfRange {
            command,
            offset,
            size,
        };
        match command {
            Command::Begin => {
                if self.transaction.is_some() {
                    self.violations.push(Violation::NestedBegin);
                } else {
                    self.transaction = Some(Vec::new());
                }
                None
            }
            Command::End => {
                match self.transaction.take() {
                    Some(writes) => {
                        for write in writes {
                            self.apply(&write);
                        }
                        self.committed += 1;
                    }
                    None => self.violations.push(Violation::EndWithoutBegin),
                }
                None
            }
            Command::ReadCapabilities | Command::ReadConfig | Command::ReadColours => {
                let memory: &[u8] = match command {
                    Command::ReadCapabilities => &self.capabilities,
                    Command::ReadConfig => &self.config,
                    _ => &self.colours,
                };
                match memory.get(start..end) {
                    Some(bytes) => Some(bytes.to_vec()),
                    None => {
                        self.violations.push(out_of_range);
                        None
                    }
                }
            }
            Command::SetParameter | Command::WriteColours => {
                let limit = if command == Command::SetParameter {
                    CONFIG_PROFILES_LEN
                } else {
                    COLOUR_SPACE
                };
                if end > limit {
                    self.violations.push(out_of_range);
                } else if let Some(staged) = self.transaction.as_mut() {
                    staged.push(StagedWrite {
                        command,
                        offset: start,
                        data: data.to_vec(),
                    });
                } else {
                    self.violations
                        .push(Violation::WriteOutsideTransaction(command));
                }
                None
            }
        }
    }

    fn apply(&mut self, write: &StagedWrite) {
        let memory: &mut [u8] = if write.command == Command::SetParameter {
            &mut self.config
        } else {
            &mut self.colours
        };
        // Bounds were checked when the write was staged.
        if let Some(target) = memory.get_mut(write.offset..write.offset + write.data.len()) {
            target.copy_from_slice(&write.data);
        }
    }
}

fn copy_prefix(target: &mut [u8], source: &[u8]) {
    let len = target.len().min(source.len());
    target[..len].copy_from_slice(&source[..len]);
}

impl Transport for SimulatedKeyboard {
    fn send(&mut self, request: &Request) -> Result<(), TransportError> {
        if !self.plugged_in {
            return Err(TransportError::Disconnected);
        }
        self.handle(request.encode());
        Ok(())
    }

    fn receive(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
        if !self.plugged_in {
            return Err(TransportError::Disconnected);
        }
        Ok(self.outbox.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris_proto::{ConfigBlock, Mode, Reply, Setting};

    const WAIT: Duration = Duration::from_millis(100);

    /// Sends a request and returns its reply, skipping foreign reports.
    fn exchange(keyboard: &mut SimulatedKeyboard, request: &Request) -> Option<Reply> {
        keyboard.send(request).unwrap();
        while let Some(report) = keyboard.receive(WAIT).unwrap() {
            if let Ok(reply) = Reply::parse(&report)
                && reply.answers(request)
            {
                return Some(reply);
            }
        }
        None
    }

    fn transaction(keyboard: &mut SimulatedKeyboard, writes: &[Request]) {
        exchange(keyboard, &Request::begin()).unwrap();
        for write in writes {
            exchange(keyboard, write).unwrap();
        }
        exchange(keyboard, &Request::end()).unwrap();
    }

    #[test]
    fn reads_reproduce_the_capture_byte_for_byte() {
        let mut keyboard = SimulatedKeyboard::reference();
        let mut pairs = vec![(capture::CAPABILITIES, Request::read_capabilities())];
        for (label, profile) in capture::CONFIG.into_iter().zip(Profile::ALL) {
            pairs.push((label, Request::read_config(profile)));
        }
        pairs.push((
            capture::COLOURS,
            Request::read_colour_map(Profile::One)[0].clone(),
        ));
        for (label, request) in pairs {
            let captured = capture::probe_reply(capture::REFERENCE_2026_10_04, label).unwrap();
            let reply = exchange(&mut keyboard, &request).unwrap();
            assert_eq!(reply.as_bytes(), &captured, "{label}");
        }
        assert!(keyboard.violations().is_empty());
    }

    #[test]
    fn colour_write_applies_at_end() {
        let mut keyboard = SimulatedKeyboard::reference();
        let green = Rgb::new(0, 255, 0);
        let write = Request::write_colours(Profile::Two, 59, &[green]).unwrap();

        exchange(&mut keyboard, &Request::begin()).unwrap();
        exchange(&mut keyboard, &write).unwrap();
        assert_eq!(keyboard.colour(Profile::Two, 59), Some(Rgb::default()));
        exchange(&mut keyboard, &Request::end()).unwrap();

        assert_eq!(keyboard.colour(Profile::Two, 59), Some(green));
        let read = Request::read_colours(Profile::Two, 59, 1).unwrap();
        assert_eq!(
            exchange(&mut keyboard, &read).unwrap().payload(),
            [0, 255, 0]
        );
        assert_eq!(keyboard.committed_transactions(), 1);
        assert!(keyboard.violations().is_empty());
    }

    #[test]
    fn full_map_round_trips() {
        let mut keyboard = SimulatedKeyboard::reference();
        let map: Vec<Rgb> = (0..COLOUR_SLOTS as u8)
            .map(|i| Rgb::new(i, !i, i / 2))
            .collect();
        transaction(
            &mut keyboard,
            &Request::write_colour_run(Profile::Three, 0, &map).unwrap(),
        );
        let mut read_back = Vec::new();
        for request in Request::read_colour_map(Profile::Three) {
            read_back.extend_from_slice(exchange(&mut keyboard, &request).unwrap().payload());
        }
        let expected: Vec<u8> = map.iter().flat_map(|c| [c.r, c.g, c.b]).collect();
        assert_eq!(read_back, expected);
        assert_eq!(keyboard.write_packets(), 7);
        assert!(keyboard.violations().is_empty());
    }

    #[test]
    fn set_parameter_applies_at_end_and_reads_back() {
        let mut keyboard = SimulatedKeyboard::reference();
        transaction(
            &mut keyboard,
            &[
                Request::set_parameter(Profile::Two, Setting::Mode(Mode::Static)),
                Request::set_parameter(Profile::Two, Setting::ModeColour(Rgb::new(1, 2, 3))),
            ],
        );
        let reply = exchange(&mut keyboard, &Request::read_config(Profile::Two)).unwrap();
        let block = ConfigBlock::parse(reply.payload()).unwrap();
        assert_eq!(block.mode(), Some(Mode::Static));
        assert_eq!(block.mode_colour(), Rgb::new(1, 2, 3));
        // Untouched bytes survive, including the undocumented byte 0x13.
        assert_eq!(block.brightness(), 4);
        assert_eq!(block.raw()[0x13], 0xFF);
        assert_eq!(keyboard.config(Profile::One)[0], Mode::SpectrumCycle.id());
        assert!(keyboard.violations().is_empty());
    }

    #[test]
    fn reads_inside_a_transaction_see_committed_state() {
        let mut keyboard = SimulatedKeyboard::reference();
        exchange(&mut keyboard, &Request::begin()).unwrap();
        exchange(
            &mut keyboard,
            &Request::set_parameter(Profile::One, Setting::Brightness(0)),
        )
        .unwrap();
        let reply = exchange(&mut keyboard, &Request::read_config(Profile::One)).unwrap();
        assert_eq!(ConfigBlock::parse(reply.payload()).unwrap().brightness(), 4);
    }

    #[test]
    fn write_outside_a_transaction_is_ignored_and_flagged() {
        let mut keyboard = SimulatedKeyboard::reference();
        let write = Request::set_parameter(Profile::One, Setting::Speed(5));
        exchange(&mut keyboard, &write).unwrap();
        assert_eq!(keyboard.config(Profile::One)[2], 0);
        assert_eq!(
            keyboard.violations(),
            [Violation::WriteOutsideTransaction(Command::SetParameter)]
        );
    }

    #[test]
    fn abandoned_transaction_applies_nothing() {
        let mut keyboard = SimulatedKeyboard::reference();
        exchange(&mut keyboard, &Request::begin()).unwrap();
        exchange(
            &mut keyboard,
            &Request::write_colours(Profile::One, 0, &[Rgb::new(0, 0, 255)]).unwrap(),
        )
        .unwrap();
        assert!(keyboard.in_transaction());
        assert_eq!(keyboard.colour(Profile::One, 0), Some(Rgb::new(255, 0, 0)));
        assert_eq!(keyboard.committed_transactions(), 0);
    }

    #[test]
    fn nested_begin_and_stray_end_are_flagged() {
        let mut keyboard = SimulatedKeyboard::reference();
        exchange(&mut keyboard, &Request::end()).unwrap();
        exchange(&mut keyboard, &Request::begin()).unwrap();
        exchange(&mut keyboard, &Request::begin()).unwrap();
        assert_eq!(
            keyboard.violations(),
            [Violation::EndWithoutBegin, Violation::NestedBegin]
        );
    }

    #[test]
    fn every_request_gets_exactly_one_reply() {
        let mut keyboard = SimulatedKeyboard::reference();
        let requests = [
            Request::read_capabilities(),
            Request::begin(),
            Request::set_parameter(Profile::One, Setting::RandomColour(true)),
            Request::end(),
        ];
        for request in &requests {
            keyboard.send(request).unwrap();
        }
        let mut replies = Vec::new();
        while let Some(report) = keyboard.receive(WAIT).unwrap() {
            replies.push(Reply::parse(&report).unwrap());
        }
        assert_eq!(replies.len(), requests.len());
        for (reply, request) in replies.iter().zip(&requests) {
            assert!(reply.answers(request));
            assert_eq!(reply.status(), 0);
        }
    }

    #[test]
    fn foreign_reports_arrive_before_the_reply() {
        let mut keyboard = SimulatedKeyboard::reference();
        let mut key_press = vec![0u8; 16];
        key_press[0] = 0x01;
        keyboard.queue_foreign_report(key_press.clone());
        keyboard.queue_foreign_report(vec![0x03, 0xe9, 0x00]);

        let request = Request::read_capabilities();
        keyboard.send(&request).unwrap();
        assert_eq!(keyboard.receive(WAIT).unwrap(), Some(key_press));
        assert_eq!(
            keyboard.receive(WAIT).unwrap(),
            Some(vec![0x03, 0xe9, 0x00])
        );
        let reply = Reply::parse(&keyboard.receive(WAIT).unwrap().unwrap()).unwrap();
        assert!(reply.answers(&request));
    }

    #[test]
    fn dropped_reply_times_out() {
        let mut keyboard = SimulatedKeyboard::reference();
        keyboard.drop_replies(1);
        assert!(exchange(&mut keyboard, &Request::read_capabilities()).is_none());
        assert!(exchange(&mut keyboard, &Request::read_capabilities()).is_some());
    }

    #[test]
    fn unplug_fails_io_and_loses_the_open_transaction() {
        let mut keyboard = SimulatedKeyboard::reference();
        exchange(&mut keyboard, &Request::begin()).unwrap();
        keyboard.unplug();
        assert!(matches!(
            keyboard.send(&Request::end()),
            Err(TransportError::Disconnected)
        ));
        assert!(matches!(
            keyboard.receive(WAIT),
            Err(TransportError::Disconnected)
        ));
        keyboard.plug_in();
        assert!(!keyboard.in_transaction());
        assert_eq!(keyboard.config(Profile::One)[0], Mode::SpectrumCycle.id());
    }

    #[test]
    fn profile_three_read_overrun_is_in_range() {
        let mut keyboard = SimulatedKeyboard::reference();
        // Profile 3's read reaches 0x8C, inside readable config space.
        exchange(&mut keyboard, &Request::read_config(Profile::Three)).unwrap();
        assert!(keyboard.violations().is_empty());
    }

    #[test]
    fn write_packets_counts_only_data_writes() {
        let mut keyboard = SimulatedKeyboard::reference();
        exchange(&mut keyboard, &Request::read_capabilities()).unwrap();
        transaction(
            &mut keyboard,
            &[Request::set_parameter(Profile::One, Setting::Brightness(4))],
        );
        assert_eq!(keyboard.write_packets(), 1);
        assert_eq!(keyboard.received().len(), 4);
    }
}
