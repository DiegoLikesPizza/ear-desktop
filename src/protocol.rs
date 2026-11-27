//! Wire format shared by every Nothing / CMF device over the SPP RFCOMM channel.
//!
//! Frame: `55 | ctrl | 01 | cmd(le16) | len(le16) | op | payload | crc16(le)`.
//! The CRC (Modbus CRC-16, poly 0xA001) is present when bit 5 of `ctrl` is set,
//! which is always the case for frames we send (`ctrl = 0x60`).

pub const SYNC: u8 = 0x55;
const HEADER_LEN: usize = 8;
const CRC_FLAG: u8 = 0x20;
const MAX_PAYLOAD: usize = 4096;

pub fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xA001 } else { crc >> 1 };
        }
    }
    crc
}

pub fn encode(command: u16, op: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(HEADER_LEN + payload.len() + 2);
    f.extend_from_slice(&[SYNC, 0x60, 0x01]);
    f.extend_from_slice(&command.to_le_bytes());
    f.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    f.push(op);
    f.extend_from_slice(payload);
    let crc = crc16(&f);
    f.extend_from_slice(&crc.to_le_bytes());
    f
}

/// A decoded incoming frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub command: u16,
    pub op: u8,
    pub payload: Vec<u8>,
}

/// `0xF0xx` commands are writes that the device acknowledges one at a time.
pub fn is_set(command: u16) -> bool {
    command & 0xFF00 == 0xF000
}

/// A response has bit 15 clear: 0xF041 is acked by 0x7041, 0xC007 answered by 0x4007.
pub fn is_reply_to(received: u16, requested: u16) -> bool {
    received & 0x8000 == 0 && (received & 0x7FFF) == (requested & 0x7FFF)
}

/// Buffers RX chunks until whole frames are available, resyncing on garbage or bad CRC.
#[derive(Default)]
pub struct Reassembler {
    buf: Vec<u8>,
}

impl Reassembler {
    pub fn push(&mut self, chunk: &[u8], mut on_frame: impl FnMut(Frame)) {
        self.buf.extend_from_slice(chunk);
        let buf = &self.buf;
        let mut off = 0;
        while off < buf.len() {
            if buf[off] != SYNC {
                off += 1;
                continue;
            }
            if buf.len() - off < HEADER_LEN {
                break;
            }
            let len = u16::from_le_bytes([buf[off + 5], buf[off + 6]]) as usize;
            if len > MAX_PAYLOAD {
                off += 1;
                continue;
            }
            let crc_len = if buf[off + 1] & CRC_FLAG != 0 { 2 } else { 0 };
            let total = HEADER_LEN + len + crc_len;
            if buf.len() - off < total {
                break;
            }
            let frame = &buf[off..off + total];
            if crc_len == 2 {
                let end = total - 2;
                if crc16(&frame[..end]) != u16::from_le_bytes([frame[end], frame[end + 1]]) {
                    off += 1;
                    continue;
                }
            }
            on_frame(Frame {
                command: u16::from_le_bytes([frame[3], frame[4]]),
                op: frame[7],
                payload: frame[HEADER_LEN..HEADER_LEN + len].to_vec(),
            });
            off += total;
        }
        self.buf.drain(..off);
    }
}

/// Floats on the wire are little-endian IEEE-754.
pub fn f32_le(v: f32) -> [u8; 4] {
    v.to_le_bytes()
}

/// Inverse of the firmware's float encoding, including its quirk of flagging tiny
/// negative values by setting the top bit of an otherwise-zero mantissa.
pub fn f32_from_wire(b: &[u8]) -> f32 {
    let mut be = [b[3], b[2], b[1], b[0]];
    if be[0] == 0 && be[1] == 0 && be[2] == 0 && be[3] & 0x80 != 0 {
        be[3] &= 0x7F;
        -f32::from_be_bytes(be)
    } else {
        f32::from_be_bytes(be)
    }
}

pub mod cmd {
    // reads (0xC0xx)
    pub const GET_BATTERY: u16 = 0xC007;
    pub const GET_IN_EAR: u16 = 0xC00E;
    pub const GET_GESTURES: u16 = 0xC018;
    pub const GET_ANC: u16 = 0xC01E;
    pub const GET_EQ: u16 = 0xC01F;
    pub const GET_PERSONAL_ANC: u16 = 0xC020;
    pub const GET_MIMI: u16 = 0xC022;
    pub const GET_DUAL_ENABLE: u16 = 0xC027;
    pub const GET_DUAL_LIST: u16 = 0xC028;
    pub const GET_CODEC: u16 = 0xC029;
    pub const GET_LATENCY: u16 = 0xC041;
    pub const GET_FIRMWARE: u16 = 0xC042;
    pub const GET_CUSTOM_EQ: u16 = 0xC044;
    pub const GET_ADVANCED_EQ: u16 = 0xC04C;
    pub const GET_ADVANCED_EQ_VALUE: u16 = 0xC04D;
    pub const GET_BASS: u16 = 0xC04E;
    pub const GET_SPATIAL: u16 = 0xC04F;
    pub const GET_LISTENING_MODE: u16 = 0xC050;
    pub const GET_AUDIODO: u16 = 0xC05A;
    pub const GET_SUPER_MIC: u16 = 0xC05E;
    pub const GET_CALL_TRANSPARENCY: u16 = 0xC05F;
    pub const GET_WALKIE_TALKIE: u16 = 0xC060;
    pub const GET_MIC_MODE: u16 = 0xC064;
    pub const GET_LONG_POWER: u16 = 0xC067;
    pub const GET_FLAT_EQ: u16 = 0xC068;
    pub const GET_ANTI_LEAKAGE: u16 = 0xC071;

    // writes (0xF0xx)
    pub const SET_RING: u16 = 0xF002;
    pub const SET_GESTURE: u16 = 0xF003;
    pub const SET_IN_EAR: u16 = 0xF004;
    pub const SET_UTC_TIME: u16 = 0xF00A;
    pub const SET_ANC: u16 = 0xF00F;
    pub const SET_EQ: u16 = 0xF010;
    pub const SET_PERSONAL_ANC: u16 = 0xF011;
    pub const SET_EAR_FIT_TEST: u16 = 0xF014;
    pub const SET_MIMI: u16 = 0xF015;
    pub const SET_DUAL_ENABLE: u16 = 0xF01A;
    pub const SET_DUAL_CONNECT: u16 = 0xF01B;
    pub const SET_CODEC: u16 = 0xF01C;
    pub const SET_LISTENING_MODE: u16 = 0xF01D;
    pub const SET_LATENCY: u16 = 0xF040;
    pub const SET_CUSTOM_EQ: u16 = 0xF041;
    pub const SET_ADVANCED_EQ: u16 = 0xF04F;
    pub const SET_ADVANCED_EQ_VALUE: u16 = 0xF050;
    pub const SET_BASS: u16 = 0xF051;
    pub const SET_SPATIAL: u16 = 0xF052;
    pub const SET_AUDIODO: u16 = 0xF05C;
    pub const SET_SUPER_MIC: u16 = 0xF05F;
    pub const SET_CALL_TRANSPARENCY: u16 = 0xF060;
    pub const SET_WALKIE_TALKIE: u16 = 0xF061;
    pub const SET_MIC_MODE: u16 = 0xF065;
    pub const SET_LONG_POWER: u16 = 0xF068;
    pub const SET_ANTI_LEAKAGE: u16 = 0xF075;

    // pushes (0xE0xx) — normalised with `| 0x8000` like replies
    pub const PUSH_BATTERY: u16 = 0xE001;
    pub const PUSH_ANC: u16 = 0xE003;
    pub const PUSH_EAR_FIT: u16 = 0xE00D;
    pub const PUSH_DUAL_EVENT: u16 = 0xE00E;
    pub const PUSH_AUDIODO: u16 = 0xE019;
    pub const PUSH_FLAT_EQ: u16 = 0xE01C;
    pub const PUSH_FIND_STATE: u16 = 0xC002;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_battery_request() {
        // matches what ear-web emits for the first request of a session
        let f = encode(cmd::GET_BATTERY, 1, &[]);
        assert_eq!(&f[..8], &[0x55, 0x60, 0x01, 0x07, 0xC0, 0x00, 0x00, 0x01]);
        let crc = crc16(&f[..8]);
        assert_eq!(&f[8..], &crc.to_le_bytes());
    }

    #[test]
    fn reassembles_split_and_merged_frames() {
        let a = encode(0x4007, 3, &[3, 2, 80, 3, 0x80 | 55, 4, 100]);
        let b = encode(0x4042, 4, b"1.0.2.3");
        let mut stream = vec![0x00, 0x13];
        stream.extend(&a);
        stream.extend(&b);
        let mut r = Reassembler::default();
        let mut got = Vec::new();
        let (x, y) = stream.split_at(5);
        r.push(x, |f| got.push(f));
        assert!(got.is_empty());
        r.push(y, |f| got.push(f));
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].command, 0x4007);
        assert_eq!(got[1].payload, b"1.0.2.3");
    }

    #[test]
    fn drops_bad_crc() {
        let mut a = encode(0x4007, 3, &[1, 2, 3]);
        let n = a.len();
        a[n - 1] ^= 0xFF;
        let good = encode(0x4041, 4, &[1]);
        a.extend(&good);
        let mut got = Vec::new();
        Reassembler::default().push(&a, |f| got.push(f));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].command, 0x4041);
    }

    #[test]
    fn float_round_trip() {
        for v in [0.0f32, 1.5, -3.0, 6.0, -6.0, 980.0] {
            assert_eq!(f32_from_wire(&f32_le(v)), v);
        }
    }

    #[test]
    fn reply_matching() {
        assert!(is_reply_to(0x7041, 0xF041));
        assert!(is_reply_to(0x4007, 0xC007));
        assert!(!is_reply_to(0xE001, 0xC007));
    }
}
