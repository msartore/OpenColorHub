//! RGB frame layout shared by every transport.
//!
//! The controller firmware fixes this layout; both the Linux `/dev/hidraw`
//! backend and the Windows HID backend build packets here so the two can never
//! drift apart.
//!
//! ```text
//! offset 0        report ID (0x00)
//! offset 1..=3    magic 0x01 0x00 0x88
//! offset 4        packet index 0..=9, or 0xFF for the commit packet
//! offset 5..     20 × (R, G, B)
//! ```
//!
//! A full update is ten packets of twenty LEDs each, followed by one commit.

use crate::model::{Channel, TOTAL_LEDS};

/// LEDs carried by one HID packet.
pub const LEDS_PER_PACKET: usize = 20;

/// Magic preamble understood by the controller firmware.
pub const PACKET_MAGIC: [u8; 3] = [0x01, 0x00, 0x88];

/// Report ID the firmware uses (0 when the descriptor declares none).
pub const REPORT_ID: u8 = 0x00;

/// Length of one packet including the leading report-ID byte.
pub const PACKET_LEN: usize = 5 + LEDS_PER_PACKET * 3;

/// Packet index that tells the controller to latch the frame.
pub const COMMIT_INDEX: u8 = 0xFF;

/// Number of packets needed for a full frame, excluding the commit.
pub const PACKET_COUNT: usize = TOTAL_LEDS / LEDS_PER_PACKET;

/// Builds one data packet. Bytes past `chunk` stay zero, matching firmware
/// behaviour for channels with fewer than twenty LEDs.
pub fn packet(index: u8, chunk: &[[u8; 3]]) -> [u8; PACKET_LEN] {
    let mut pkt = [0u8; PACKET_LEN];
    pkt[0] = REPORT_ID;
    pkt[1] = PACKET_MAGIC[0];
    pkt[2] = PACKET_MAGIC[1];
    pkt[3] = PACKET_MAGIC[2];
    pkt[4] = index;
    for (i, rgb) in chunk.iter().enumerate().take(LEDS_PER_PACKET) {
        let base = 5 + i * 3;
        pkt[base] = rgb[0];
        pkt[base + 1] = rgb[1];
        pkt[base + 2] = rgb[2];
    }
    pkt
}

/// The latch packet that commits the frame that was just streamed.
pub fn commit() -> [u8; PACKET_LEN] {
    packet(COMMIT_INDEX, &[])
}

/// Expands a flat LED array into the ten data packets followed by the commit.
pub fn frame(leds: &[[u8; 3]]) -> Vec<[u8; PACKET_LEN]> {
    let mut out: Vec<[u8; PACKET_LEN]> = leds
        .chunks(LEDS_PER_PACKET)
        .enumerate()
        .map(|(i, chunk)| packet(i as u8, chunk))
        .collect();
    out.push(commit());
    out
}

/// Channels covered by the first packet (onboard LEDs), useful for the CLI.
pub fn first_packet_channels() -> &'static [Channel] {
    &[Channel::Led]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_holds_twenty_leds() {
        assert_eq!(PACKET_LEN, 65);
        assert_eq!(LEDS_PER_PACKET * 3 + 5, PACKET_LEN);
    }

    #[test]
    fn total_leds_divide_evenly_into_packets() {
        assert_eq!(PACKET_COUNT, 10);
        assert_eq!(TOTAL_LEDS % LEDS_PER_PACKET, 0);
    }

    #[test]
    fn header_bytes_are_exact() {
        let pkt = packet(3, &[[0xAA, 0xBB, 0xCC]]);
        assert_eq!(&pkt[0..5], &[0x00, 0x01, 0x00, 0x88, 0x03]);
        assert_eq!(&pkt[5..8], &[0xAA, 0xBB, 0xCC]);
        // The rest of the packet stays zeroed.
        assert!(pkt[8..].iter().all(|b| *b == 0));
    }

    #[test]
    fn commit_packet_uses_the_sentinel_index() {
        let pkt = commit();
        assert_eq!(pkt[4], 0xFF);
        assert!(pkt[5..].iter().all(|b| *b == 0));
    }

    #[test]
    fn frame_is_ten_packets_plus_commit() {
        let leds = [[0x10u8, 0x20, 0x30]; TOTAL_LEDS];
        let packets = frame(&leds);
        assert_eq!(packets.len(), PACKET_COUNT + 1);
        assert_eq!(packets[0][4], 0);
        assert_eq!(packets[9][4], 9);
        assert_eq!(packets[10][4], COMMIT_INDEX);
    }

    #[test]
    fn short_frames_are_padded_not_dropped() {
        // A caller that supplies fewer LEDs than the controller expects still
        // gets a well-formed packet stream: 20 + 5 LEDs across two packets.
        let leds = [[1u8, 2, 3]; 25];
        let packets = frame(&leds);
        assert_eq!(packets.len(), 3); // 2 data packets + commit
        assert_eq!(
            &packets[1][5..20],
            &[1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 3]
        );
        assert!(packets[1][20..].iter().all(|b| *b == 0));
    }

    #[test]
    fn onboard_leds_fit_in_the_probe_packet() {
        assert!(
            Channel::Led.led_range().end <= LEDS_PER_PACKET,
            "onboard LEDs must fit in packet 0"
        );
    }
}
