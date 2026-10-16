//! What a [`Lan`](crate::Lan) runs on: a clock and datagram sockets. The
//! game uses [`Udp`], real sockets and the wall clock. Tests use
//! [`MemoryNet`], a network inside the process on a clock they move by
//! hand, so timeouts, delay changes, lost, late, doubled and reordered
//! packets play out the same way every run, and fast (review fix R14).

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// A clock and a way to open sockets.
pub trait Net: Send {
    fn now(&self) -> Instant;
    /// Wait a while (only a blocking join waits).
    fn sleep(&self, d: Duration);
    /// A non-blocking socket on `port` of every address (0 picks a free
    /// port), able to broadcast.
    fn bind(&self, port: u16) -> io::Result<Box<dyn Socket>>;
}

/// A datagram socket that never blocks: `recv_from` fails with
/// [`ErrorKind::WouldBlock`] when nothing is waiting.
pub trait Socket: Send {
    fn send_to(&self, bytes: &[u8], to: SocketAddr) -> io::Result<usize>;
    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
}

/// Real UDP and the wall clock.
pub struct Udp;

impl Net for Udp {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, d: Duration) {
        std::thread::sleep(d);
    }

    fn bind(&self, port: u16) -> io::Result<Box<dyn Socket>> {
        let s = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))?;
        s.set_nonblocking(true)?;
        let _ = s.set_broadcast(true);
        Ok(Box::new(s))
    }
}

impl Socket for UdpSocket {
    fn send_to(&self, bytes: &[u8], to: SocketAddr) -> io::Result<usize> {
        UdpSocket::send_to(self, bytes, to)
    }

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        UdpSocket::recv_from(self, buf)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        UdpSocket::local_addr(self)
    }
}

/// Packets a socket holds unread before it drops more, as a real socket's
/// buffer would.
const INBOX: usize = 512;

/// First port handed out for port 0.
const FIRST_FREE_PORT: u16 = 50_000;

/// A network inside the process, shared by every [`Lan`](crate::Lan) a
/// test makes from clones of it. Every socket is `127.0.0.1:port` and a
/// packet goes to the socket on its port whatever address it was sent to,
/// so broadcasts and join codes reach it too. Time stands still until
/// [`MemoryNet::advance`]; a packet arrives after the network's latency,
/// unless it is lost, and may arrive twice.
#[derive(Clone)]
pub struct MemoryNet(Arc<Mutex<Bus>>);

struct Bus {
    start: Instant,
    now: Duration,
    rng: u64,
    /// Each packet takes between these to arrive (a spread reorders them).
    latency: (Duration, Duration),
    /// Percentages of packets lost and doubled.
    loss: u32,
    duplicate: u32,
    sockets: HashMap<u16, VecDeque<(Vec<u8>, SocketAddr)>>,
    /// Ports cut off from the network: what they send and what is sent to
    /// them is lost.
    cut: HashSet<u16>,
    next_port: u16,
    flight: Vec<Packet>,
    /// Packets put on the network so far, to keep ones arriving together
    /// in the order they were sent.
    seq: u64,
}

/// A packet on its way.
struct Packet {
    arrival: Duration,
    seq: u64,
    to: u16,
    from: SocketAddr,
    bytes: Vec<u8>,
}

impl MemoryNet {
    /// A network with a 1 ms latency, losing nothing, its randomness from
    /// `seed`.
    pub fn new(seed: u64) -> MemoryNet {
        MemoryNet(Arc::new(Mutex::new(Bus {
            start: Instant::now(),
            now: Duration::ZERO,
            rng: seed | 1,
            latency: (Duration::from_millis(1), Duration::from_millis(1)),
            loss: 0,
            duplicate: 0,
            sockets: HashMap::new(),
            cut: HashSet::new(),
            next_port: FIRST_FREE_PORT,
            flight: Vec::new(),
            seq: 0,
        })))
    }

    fn bus(&self) -> MutexGuard<'_, Bus> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Each packet takes from `min` to `max` to arrive.
    pub fn set_latency(&self, min: Duration, max: Duration) {
        self.bus().latency = (min, max.max(min));
    }

    /// Lose `percent` of packets.
    pub fn set_loss(&self, percent: u32) {
        self.bus().loss = percent.min(100);
    }

    /// Deliver `percent` of packets twice.
    pub fn set_duplicate(&self, percent: u32) {
        self.bus().duplicate = percent.min(100);
    }

    /// Cut the socket on `port` off the network, or put it back.
    pub fn cut(&self, port: u16, cut: bool) {
        let mut b = self.bus();
        if cut {
            b.cut.insert(port);
        } else {
            b.cut.remove(&port);
        }
    }

    /// Move the clock on.
    pub fn advance(&self, d: Duration) {
        self.bus().now += d;
    }

    /// Put `bytes` on the network as if sent from `from` (a stranger, or a
    /// forger).
    pub fn inject(&self, bytes: &[u8], from: SocketAddr, to: SocketAddr) {
        self.bus().send(bytes, from, to);
    }
}

impl Bus {
    fn roll(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn chance(&mut self, percent: u32) -> bool {
        percent > 0 && self.roll() % 100 < u64::from(percent)
    }

    fn send(&mut self, bytes: &[u8], from: SocketAddr, to: SocketAddr) {
        if self.cut.contains(&from.port()) || self.cut.contains(&to.port()) {
            return;
        }
        let copies = if self.chance(self.loss) {
            0
        } else if self.chance(self.duplicate) {
            2
        } else {
            1
        };
        for _ in 0..copies {
            let (min, max) = self.latency;
            let spread = (max - min).as_micros() as u64;
            let delay = min + Duration::from_micros(self.roll() % (spread + 1));
            self.seq += 1;
            self.flight.push(Packet {
                arrival: self.now + delay,
                seq: self.seq,
                to: to.port(),
                from,
                bytes: bytes.to_vec(),
            });
        }
    }

    /// Move every packet that has arrived into its socket's inbox, in the
    /// order they arrived.
    fn deliver(&mut self) {
        let now = self.now;
        let (mut arrived, flying): (Vec<Packet>, Vec<Packet>) = std::mem::take(&mut self.flight)
            .into_iter()
            .partition(|p| p.arrival <= now);
        self.flight = flying;
        arrived.sort_by_key(|p| (p.arrival, p.seq));
        for p in arrived {
            if self.cut.contains(&p.to) {
                continue;
            }
            if let Some(inbox) = self.sockets.get_mut(&p.to)
                && inbox.len() < INBOX
            {
                inbox.push_back((p.bytes, p.from));
            }
        }
    }
}

impl Net for MemoryNet {
    fn now(&self) -> Instant {
        let b = self.bus();
        b.start + b.now
    }

    fn sleep(&self, d: Duration) {
        self.advance(d);
    }

    fn bind(&self, port: u16) -> io::Result<Box<dyn Socket>> {
        let mut b = self.bus();
        let port = if port == 0 {
            while b.sockets.contains_key(&b.next_port) {
                b.next_port = b.next_port.wrapping_add(1).max(FIRST_FREE_PORT);
            }
            b.next_port
        } else {
            port
        };
        if b.sockets.contains_key(&port) {
            return Err(io::Error::from(ErrorKind::AddrInUse));
        }
        b.sockets.insert(port, VecDeque::new());
        Ok(Box::new(MemorySocket {
            net: self.clone(),
            addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
        }))
    }
}

struct MemorySocket {
    net: MemoryNet,
    addr: SocketAddr,
}

impl Socket for MemorySocket {
    fn send_to(&self, bytes: &[u8], to: SocketAddr) -> io::Result<usize> {
        self.net.bus().send(bytes, self.addr, to);
        Ok(bytes.len())
    }

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let mut b = self.net.bus();
        b.deliver();
        let inbox = b
            .sockets
            .get_mut(&self.addr.port())
            .ok_or_else(|| io::Error::from(ErrorKind::NotConnected))?;
        let (bytes, from) = inbox
            .pop_front()
            .ok_or_else(|| io::Error::from(ErrorKind::WouldBlock))?;
        // A datagram longer than the buffer is cut, as UDP does.
        let n = bytes.len().min(buf.len());
        buf[..n].copy_from_slice(&bytes[..n]);
        Ok((n, from))
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.addr)
    }
}

impl Drop for MemorySocket {
    fn drop(&mut self) {
        self.net.bus().sockets.remove(&self.addr.port());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recv(s: &dyn Socket) -> Option<Vec<u8>> {
        let mut buf = [0u8; 64];
        s.recv_from(&mut buf).ok().map(|(n, _)| buf[..n].to_vec())
    }

    #[test]
    fn packets_arrive_after_the_latency_in_order() {
        let net = MemoryNet::new(1);
        net.set_latency(Duration::from_millis(5), Duration::from_millis(5));
        let a = net.bind(0).unwrap();
        let b = net.bind(0).unwrap();
        let to = b.local_addr().unwrap();
        a.send_to(b"one", to).unwrap();
        a.send_to(b"two", to).unwrap();
        net.advance(Duration::from_millis(4));
        assert_eq!(recv(&*b), None);
        net.advance(Duration::from_millis(1));
        assert_eq!(recv(&*b).unwrap(), b"one");
        assert_eq!(recv(&*b).unwrap(), b"two");
        assert_eq!(recv(&*b), None);
    }

    #[test]
    fn a_spread_of_latencies_reorders_and_loss_and_doubles_count_up() {
        let net = MemoryNet::new(7);
        net.set_latency(Duration::from_millis(1), Duration::from_millis(30));
        net.set_loss(20);
        net.set_duplicate(10);
        let a = net.bind(0).unwrap();
        let b = net.bind(0).unwrap();
        let to = b.local_addr().unwrap();
        for i in 0..200u8 {
            a.send_to(&[i], to).unwrap();
        }
        net.advance(Duration::from_millis(30));
        let got: Vec<u8> = std::iter::from_fn(|| recv(&*b)).map(|p| p[0]).collect();
        assert!(got.windows(2).any(|w| w[0] > w[1]), "never reordered");
        assert!(got.len() < 200, "nothing lost");
        let mut seen = got.clone();
        seen.sort_unstable();
        seen.dedup();
        assert!(seen.len() < got.len(), "nothing doubled");
    }

    #[test]
    fn ports_are_taken_once_and_freed_on_drop() {
        let net = MemoryNet::new(1);
        let s = net.bind(4000).unwrap();
        assert_eq!(
            net.bind(4000).err().map(|e| e.kind()),
            Some(ErrorKind::AddrInUse)
        );
        drop(s);
        assert!(net.bind(4000).is_ok());
    }

    #[test]
    fn a_cut_socket_hears_nothing_and_says_nothing() {
        let net = MemoryNet::new(1);
        let a = net.bind(0).unwrap();
        let b = net.bind(0).unwrap();
        let (to_a, to_b) = (a.local_addr().unwrap(), b.local_addr().unwrap());
        net.cut(to_b.port(), true);
        a.send_to(b"x", to_b).unwrap();
        b.send_to(b"y", to_a).unwrap();
        net.advance(Duration::from_millis(5));
        assert_eq!(recv(&*a), None);
        assert_eq!(recv(&*b), None);
        net.cut(to_b.port(), false);
        a.send_to(b"z", to_b).unwrap();
        net.advance(Duration::from_millis(5));
        assert_eq!(recv(&*b).unwrap(), b"z");
    }
}
