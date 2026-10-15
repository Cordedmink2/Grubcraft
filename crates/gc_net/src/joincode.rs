//! Join codes: a host's address on the local network as eight letters and
//! digits a friend can type ("K7QD-M2XA"), with a check that catches typos.
//!
//! A code holds the host's IPv4 address, which of the four game ports it
//! is on, and six check bits: 40 bits, eight characters of Crockford's
//! base 32 (no I, L, O or U, so nothing reads as another character).

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};

use crate::lan::{DEFAULT_PORT, PORTS};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn check(ip: u32, port_index: u32) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(b"grubcraft join code");
    h.update(&ip.to_le_bytes());
    h.update(&[port_index as u8]);
    (h.finalize().as_bytes()[0] & 0x3f) as u64
}

/// The code for a host at `ip` on `port`; `None` for a port that isn't a
/// game port.
pub fn encode(ip: Ipv4Addr, port: u16) -> Option<String> {
    let index = port.checked_sub(DEFAULT_PORT).filter(|&i| i < PORTS)? as u32;
    let ip = u32::from(ip);
    let bits = ((ip as u64) << 8) | ((index as u64) << 6) | check(ip, index);
    let chars: String = (0..8)
        .rev()
        .map(|i| ALPHABET[((bits >> (i * 5)) & 31) as usize] as char)
        .collect();
    Some(format!("{}-{}", &chars[..4], &chars[4..]))
}

/// The address in a join code, or `None` if it isn't one (or has a typo).
/// Case, spaces and dashes don't matter, and the letters O, I and L are
/// read as the digits 0, 1 and 1.
pub fn decode(code: &str) -> Option<SocketAddr> {
    let mut bits = 0u64;
    let mut n = 0;
    for c in code.chars().filter(|c| !matches!(c, '-' | ' ')) {
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        let v = ALPHABET.iter().position(|&a| a as char == c)?;
        bits = (bits << 5) | v as u64;
        n += 1;
    }
    if n != 8 {
        return None;
    }
    let ip = (bits >> 8) as u32;
    let index = ((bits >> 6) & 3) as u32;
    if bits & 0x3f != check(ip, index) {
        return None;
    }
    Some(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::from(ip),
        DEFAULT_PORT + index as u16,
    )))
}

/// This machine's address on the local network: the one it would send
/// from to reach beyond it (nothing is sent to find out).
pub fn local_ip() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    s.connect(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 1), 9))
        .ok()?;
    match s.local_addr().ok()? {
        SocketAddr::V4(a) if !a.ip().is_unspecified() && !a.ip().is_loopback() => Some(*a.ip()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for (ip, port) in [
            (Ipv4Addr::new(192, 168, 1, 20), DEFAULT_PORT),
            (Ipv4Addr::new(10, 0, 0, 7), DEFAULT_PORT + 3),
            (Ipv4Addr::new(255, 255, 255, 255), DEFAULT_PORT + 1),
            (Ipv4Addr::new(0, 0, 0, 0), DEFAULT_PORT),
        ] {
            let code = encode(ip, port).unwrap();
            assert_eq!(code.len(), 9, "{code}");
            assert_eq!(decode(&code), Some(SocketAddr::from((ip, port))));
            assert_eq!(
                decode(&code.to_lowercase().replace('-', " ")),
                decode(&code)
            );
        }
        assert_eq!(encode(Ipv4Addr::LOCALHOST, 80), None);
    }

    #[test]
    fn typos_are_caught() {
        let code = encode(Ipv4Addr::new(192, 168, 1, 20), DEFAULT_PORT).unwrap();
        let mut caught = 0;
        let mut tries = 0;
        for i in [0, 1, 2, 3, 5, 6, 7, 8] {
            for &c in ALPHABET.iter() {
                if code.as_bytes()[i] == c {
                    continue;
                }
                let mut typo = code.clone().into_bytes();
                typo[i] = c;
                tries += 1;
                if decode(std::str::from_utf8(&typo).unwrap()).is_none() {
                    caught += 1;
                }
            }
        }
        // Six check bits miss about one typo in 64.
        assert!(caught * 100 > tries * 95, "{caught} of {tries}");
        assert_eq!(decode("K7QD-M2X"), None);
        assert_eq!(decode("192.168.1.20"), None);
        assert_eq!(decode("mario-macbook"), None);
    }
}
