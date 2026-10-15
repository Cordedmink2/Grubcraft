//! Securing a link (docs/11-networking.md, "Transport"): an X25519 key
//! exchange when a player joins, then ChaCha20-Poly1305 on every link
//! packet.
//!
//! Each side makes a fresh key pair per connection and they swap public
//! keys in the clear (the Hi packets). Both then derive one key for each
//! direction from the shared secret, both public keys and the lobby
//! password (empty when there is none), with BLAKE3. Someone without the
//! password can't make keys that work, so a joiner who typed it wrong is
//! found at their first packet. Every packet carries its number in that
//! direction as the nonce; the packet header is authenticated too, and an
//! old packet sent again is refused.
//!
//! What this does and doesn't stop: anyone just listening on the network
//! sees nothing readable. The key swap itself isn't authenticated, though,
//! so a machine actively sitting in the middle could read a game that has
//! no password, and could try guesses at a password it captured offline.
//! A password-authenticated exchange (CPace) would close that later.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use x25519_dalek::{PublicKey, StaticSecret};

/// Bytes a sealed packet adds: its 8-byte number and the 16-byte tag.
pub const OVERHEAD: usize = 8 + 16;

const CONTEXT: &str = "grubcraft 2026-10-15 lan link keys v1";

/// This side's half of a key exchange.
pub struct KeyPair {
    secret: StaticSecret,
    pub public: [u8; 32],
}

impl KeyPair {
    /// A new random key pair. `None` if the system has no randomness to
    /// give (practically never).
    pub fn new() -> Option<KeyPair> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).ok()?;
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret).to_bytes();
        Some(KeyPair { secret, public })
    }
}

/// The keys of one secured link.
pub struct Cipher {
    send: ChaCha20Poly1305,
    recv: ChaCha20Poly1305,
    /// The next number to send.
    sent: u64,
    /// The lowest number still accepted: anything older is a replay.
    next_recv: u64,
}

impl Cipher {
    /// Keys for this side, given the other side's public key. `None` if
    /// that key is degenerate (a peer trying to force a known secret).
    pub fn new(mine: &KeyPair, theirs: [u8; 32], host: bool, password: &str) -> Option<Cipher> {
        let shared = mine.secret.diffie_hellman(&PublicKey::from(theirs));
        if !shared.was_contributory() {
            return None;
        }
        let (host_pub, joiner_pub) = if host {
            (mine.public, theirs)
        } else {
            (theirs, mine.public)
        };
        let key = |direction: &[u8]| {
            let mut h = blake3::Hasher::new_derive_key(CONTEXT);
            h.update(direction);
            h.update(shared.as_bytes());
            h.update(&host_pub);
            h.update(&joiner_pub);
            h.update(password.as_bytes());
            ChaCha20Poly1305::new(Key::from_slice(h.finalize().as_bytes()))
        };
        let (to_joiner, to_host) = (key(b"host to joiner"), key(b"joiner to host"));
        let (send, recv) = if host {
            (to_joiner, to_host)
        } else {
            (to_host, to_joiner)
        };
        Some(Cipher {
            send,
            recv,
            sent: 0,
            next_recv: 0,
        })
    }

    /// Whether any packet from the other side has opened yet: once one
    /// has, the password matched.
    pub fn has_opened(&self) -> bool {
        self.next_recv > 0
    }

    fn nonce(number: u64) -> Nonce {
        let mut n = [0u8; 12];
        n[..8].copy_from_slice(&number.to_le_bytes());
        *Nonce::from_slice(&n)
    }

    /// `body` sealed, to follow `header` in a packet.
    pub fn seal(&mut self, header: &[u8], body: &[u8]) -> Vec<u8> {
        let number = self.sent;
        self.sent += 1;
        let sealed = self
            .send
            .encrypt(
                &Self::nonce(number),
                Payload {
                    msg: body,
                    aad: header,
                },
            )
            .expect("a packet always fits ChaCha20-Poly1305");
        let mut out = Vec::with_capacity(8 + sealed.len());
        out.extend_from_slice(&number.to_le_bytes());
        out.extend_from_slice(&sealed);
        out
    }

    /// The body of a sealed packet, or `None` if it was not sealed with
    /// the other side's key, was changed on the way, or is older than the
    /// last packet opened (a replay; links resend what matters anyway).
    pub fn open(&mut self, header: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < OVERHEAD {
            return None;
        }
        let number = u64::from_le_bytes(sealed[..8].try_into().ok()?);
        if number < self.next_recv {
            return None;
        }
        let body = self
            .recv
            .decrypt(
                &Self::nonce(number),
                Payload {
                    msg: &sealed[8..],
                    aad: header,
                },
            )
            .ok()?;
        self.next_recv = number + 1;
        Some(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(host_password: &str, joiner_password: &str) -> (Cipher, Cipher) {
        let h = KeyPair::new().unwrap();
        let j = KeyPair::new().unwrap();
        (
            Cipher::new(&h, j.public, true, host_password).unwrap(),
            Cipher::new(&j, h.public, false, joiner_password).unwrap(),
        )
    }

    #[test]
    fn both_ways_round_trip() {
        let (mut host, mut joiner) = pair("", "");
        let p = joiner.seal(b"GC", b"hello host");
        assert_eq!(host.open(b"GC", &p).unwrap(), b"hello host");
        let p = host.seal(b"GC", b"hello joiner");
        assert_eq!(joiner.open(b"GC", &p).unwrap(), b"hello joiner");
        // A side can't read its own packets back: the directions differ.
        let p = host.seal(b"GC", b"echo");
        assert_eq!(host.open(b"GC", &p), None);
        assert_eq!(p.len(), 4 + OVERHEAD);
    }

    #[test]
    fn tampering_and_wrong_passwords_are_refused() {
        let (mut host, mut joiner) = pair("moles", "moles");
        let mut p = joiner.seal(b"GC", b"orders");
        assert_eq!(host.open(b"GX", &p), None, "header is authenticated");
        assert!(host.open(b"GC", &p).is_some());
        let last = p.len() - 1;
        p[last] ^= 1;
        assert_eq!(host.open(b"GC", &p), None);
        assert_eq!(host.open(b"GC", &p[..10]), None);

        let (mut host, mut joiner) = pair("moles", "voles");
        assert_eq!(host.open(b"GC", &joiner.seal(b"GC", b"hi")), None);
    }

    #[test]
    fn replays_are_refused() {
        let (mut host, mut joiner) = pair("", "");
        let first = joiner.seal(b"GC", b"one");
        let second = joiner.seal(b"GC", b"two");
        assert!(host.open(b"GC", &second).is_some());
        assert_eq!(host.open(b"GC", &second), None);
        assert_eq!(host.open(b"GC", &first), None, "older than the last");
        assert!(host.open(b"GC", &joiner.seal(b"GC", b"three")).is_some());
    }

    #[test]
    fn a_zero_key_is_refused() {
        let k = KeyPair::new().unwrap();
        assert!(Cipher::new(&k, [0; 32], true, "").is_none());
    }
}
