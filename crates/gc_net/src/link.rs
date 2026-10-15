//! A reliable, ordered stream of messages over UDP between the host and one
//! player (docs/11-networking.md, "Transport").
//!
//! Redundancy instead of retransmission: every message gets the next number
//! in the stream, and every packet repeats, oldest first, all the messages
//! the other side hasn't acknowledged yet (as many as fit in a packet).
//! Each packet acknowledges by saying how many messages of the other
//! side's stream have arrived in order. A lost packet costs nothing extra:
//! the next one, 50 ms later at most during a match, carries the same
//! messages again. No timers, no retransmit stalls on bursty Wi-Fi.
//!
//! Every packet also carries a timestamp and an echo of the last one heard,
//! so both ends measure the round trip without separate pings.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::proto::{Kind, MAX_MESSAGE, MAX_PACKET, Msg, header};
use crate::wire::{Reader, Writer};

pub struct Link {
    epoch: Instant,
    /// Messages sent but not yet acknowledged, by stream number from
    /// `acked`.
    out: VecDeque<Vec<u8>>,
    /// The other side has every message numbered below this.
    acked: u64,
    /// We have every message of the other side's stream below this.
    received: u64,
    /// The other side's latest timestamp, and when it arrived.
    echo: Option<(u32, Instant)>,
    /// Smoothed round trip, if measured.
    rtt: Option<Duration>,
    /// Smoothed difference between round trips and the average.
    jitter: Duration,
    pub last_heard: Instant,
    pub last_sent: Instant,
    /// Messages queued since the last packet went out.
    pub fresh: bool,
}

impl Link {
    pub fn new(now: Instant) -> Link {
        Link {
            epoch: now,
            out: VecDeque::new(),
            acked: 0,
            received: 0,
            echo: None,
            rtt: None,
            jitter: Duration::ZERO,
            last_heard: now,
            last_sent: now,
            fresh: false,
        }
    }

    /// Queue a message for reliable, in-order delivery.
    pub fn send(&mut self, msg: &Msg) {
        let bytes = msg.encode();
        debug_assert!(bytes.len() <= MAX_MESSAGE, "{} bytes: {msg:?}", bytes.len());
        self.out.push_back(bytes);
        self.fresh = true;
    }

    /// Messages still waiting to be acknowledged.
    pub fn backlog(&self) -> usize {
        self.out.len()
    }

    /// Round trip time, once measured.
    pub fn rtt(&self) -> Option<Duration> {
        self.rtt
    }

    /// How much round trips vary around [`Link::rtt`] (the smoothed mean
    /// deviation, as TCP keeps it).
    pub fn jitter(&self) -> Duration {
        self.jitter
    }

    fn millis(&self, now: Instant) -> u32 {
        now.duration_since(self.epoch).as_millis() as u32
    }

    /// The next packet to send: header, timing, acknowledgement and as many
    /// unacknowledged messages as fit.
    pub fn packet(&mut self, now: Instant) -> Vec<u8> {
        let mut w = Writer::new();
        header(&mut w, Kind::Link);
        w.var(self.millis(now) as u64);
        match self.echo {
            Some((stamp, at)) => {
                w.var(stamp as u64 + 1);
                w.var(now.duration_since(at).as_millis() as u64);
            }
            None => w.var(0),
        }
        w.var(self.received);
        w.var(self.acked);
        let count_at = w.buf.len();
        // Room for the count byte; fewer than 128 messages ever fit.
        w.u8(0);
        let mut count = 0u8;
        for m in &self.out {
            if w.buf.len() + m.len() + 2 > MAX_PACKET {
                break;
            }
            w.bytes(m);
            count += 1;
        }
        w.buf[count_at] = count;
        self.last_sent = now;
        self.fresh = false;
        w.buf
    }

    /// Take in a link packet (after its header), returning the new messages
    /// in order. `None` if it was malformed.
    pub fn receive(&mut self, r: &mut Reader, now: Instant) -> Option<Vec<Msg>> {
        let stamp = r.var_as::<u32>()?;
        let echo = r.var()?;
        let echo_age = if echo > 0 { r.var()? } else { 0 };
        let their_received = r.var()?;
        let first = r.var()?;
        let count = r.u8()?;
        let mut msgs = Vec::new();
        for number in (first..).take(count as usize) {
            let bytes = r.bytes()?;
            if number == self.received {
                msgs.push(Msg::decode(bytes)?);
                self.received += 1;
            }
        }
        if !r.is_empty() {
            return None;
        }
        // Acknowledged messages leave the queue.
        if their_received > self.acked {
            let n = (their_received - self.acked).min(self.out.len() as u64);
            self.out.drain(..n as usize);
            self.acked += n;
        }
        if echo > 0 {
            let sent = Duration::from_millis(echo - 1);
            let held = Duration::from_millis(echo_age);
            let elapsed = now.duration_since(self.epoch);
            if let Some(sample) = elapsed.checked_sub(sent + held) {
                self.rtt = Some(match self.rtt {
                    Some(old) => {
                        let off = sample.abs_diff(old);
                        self.jitter = (self.jitter * 3 + off) / 4;
                        (old * 7 + sample) / 8
                    }
                    None => {
                        self.jitter = sample / 2;
                        sample
                    }
                });
            }
        }
        if self.echo.is_none_or(|(s, _)| stamp >= s) {
            self.echo = Some((stamp, now));
        }
        self.last_heard = now;
        Some(msgs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::read_header;

    fn deliver(packet: &[u8], to: &mut Link, now: Instant) -> Vec<Msg> {
        let mut r = Reader::new(packet);
        read_header(&mut r).unwrap();
        to.receive(&mut r, now).unwrap()
    }

    fn notice(i: usize) -> Msg {
        Msg::Notice {
            text: format!("message {i}"),
        }
    }

    #[test]
    fn messages_arrive_in_order_once_despite_losses() {
        let t0 = Instant::now();
        let mut a = Link::new(t0);
        let mut b = Link::new(t0);
        let mut got = Vec::new();
        let mut next = 0;
        for step in 0..400u32 {
            let now = t0 + Duration::from_millis(step as u64 * 10);
            if step % 3 == 0 && next < 200 {
                a.send(&notice(next));
                next += 1;
            }
            let p = a.packet(now);
            // Lose two packets in five each way, in bursts.
            if step % 5 >= 2 {
                got.extend(deliver(&p, &mut b, now));
            }
            let back = b.packet(now);
            if step % 5 != 1 && step % 7 != 0 {
                deliver(&back, &mut a, now);
            }
        }
        // One clean exchange settles the last acknowledgement.
        let end = t0 + Duration::from_secs(5);
        got.extend(deliver(&a.packet(end), &mut b, end));
        deliver(&b.packet(end), &mut a, end);
        let want: Vec<Msg> = (0..next).map(notice).collect();
        assert_eq!(got, want);
        assert_eq!(a.backlog(), 0);
    }

    #[test]
    fn a_big_backlog_spreads_over_packets() {
        let t0 = Instant::now();
        let mut a = Link::new(t0);
        let mut b = Link::new(t0);
        let big = Msg::Notice {
            text: "x".repeat(900),
        };
        for _ in 0..5 {
            a.send(&big);
        }
        let mut got = 0;
        for _ in 0..10 {
            let p = a.packet(t0);
            assert!(p.len() <= MAX_PACKET);
            got += deliver(&p, &mut b, t0).len();
            deliver(&b.packet(t0), &mut a, t0);
        }
        assert_eq!(got, 5);
    }

    #[test]
    fn round_trip_is_measured() {
        let t0 = Instant::now();
        let mut a = Link::new(t0);
        let mut b = Link::new(t0);
        let p = a.packet(t0);
        deliver(&p, &mut b, t0 + Duration::from_millis(20));
        // b holds the echo 5 ms before answering.
        let back = b.packet(t0 + Duration::from_millis(25));
        deliver(&back, &mut a, t0 + Duration::from_millis(45));
        let rtt = a.rtt().unwrap().as_millis();
        assert!((39..=41).contains(&rtt), "{rtt}");
    }

    #[test]
    fn jitter_grows_when_round_trips_vary() {
        let t0 = Instant::now();
        let mut a = Link::new(t0);
        let mut b = Link::new(t0);
        let mut now = t0;
        let mut exchange = |a: &mut Link, b: &mut Link, trip: u64| {
            let p = a.packet(now);
            deliver(&p, b, now + Duration::from_millis(trip / 2));
            let back = b.packet(now + Duration::from_millis(trip / 2));
            deliver(&back, a, now + Duration::from_millis(trip));
            now += Duration::from_millis(50);
        };
        for _ in 0..40 {
            exchange(&mut a, &mut b, 20);
        }
        let steady = a.jitter();
        assert!(steady < Duration::from_millis(2), "{steady:?}");
        for i in 0..40 {
            exchange(&mut a, &mut b, if i % 2 == 0 { 10 } else { 70 });
        }
        assert!(a.jitter() > Duration::from_millis(15), "{:?}", a.jitter());
    }
}
