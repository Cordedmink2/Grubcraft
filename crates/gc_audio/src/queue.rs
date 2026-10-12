//! A lock-free single-producer, single-consumer queue of audio commands.
//!
//! The main thread pushes; the audio thread pops. Neither blocks or
//! allocates after setup. Each command is packed into one `u64`, so the ring
//! is a plain array of atomics and needs no `unsafe`.

use crate::bank::SoundId;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// What the main thread can ask of the mixer. Volumes and gains are 0..=255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Start a sound. `pan` is -127 (left) to 127 (right); a higher
    /// `priority` survives when all voices are busy.
    Play {
        sound: SoundId,
        gain: u8,
        pan: i8,
        priority: u8,
    },
    /// 0 = calm music only, 255 = full battle layer.
    MusicIntensity(u8),
    /// 0 stops the music; anything else starts it at that volume.
    MusicVolume(u8),
    /// Overall volume.
    MasterVolume(u8),
    /// Silence every sound effect now.
    StopEffects,
}

impl Command {
    fn pack(self) -> u64 {
        match self {
            Command::Play {
                sound,
                gain,
                pan,
                priority,
            } => {
                1 | (sound as u64) << 8
                    | (gain as u64) << 16
                    | (pan as u8 as u64) << 24
                    | (priority as u64) << 32
            }
            Command::MusicIntensity(v) => 2 | (v as u64) << 8,
            Command::MusicVolume(v) => 3 | (v as u64) << 8,
            Command::MasterVolume(v) => 4 | (v as u64) << 8,
            Command::StopEffects => 5,
        }
    }

    fn unpack(word: u64) -> Option<Command> {
        let byte = |shift: u32| (word >> shift) as u8;
        Some(match byte(0) {
            1 => Command::Play {
                sound: SoundId::from_u8(byte(8))?,
                gain: byte(16),
                pan: byte(24) as i8,
                priority: byte(32),
            },
            2 => Command::MusicIntensity(byte(8)),
            3 => Command::MusicVolume(byte(8)),
            4 => Command::MasterVolume(byte(8)),
            5 => Command::StopEffects,
            _ => return None,
        })
    }
}

struct Ring {
    slots: Box<[AtomicU64]>,
    /// Next slot to read (moved only by the consumer).
    head: AtomicUsize,
    /// Next slot to write (moved only by the producer).
    tail: AtomicUsize,
}

pub struct Producer(Arc<Ring>);
pub struct Consumer(Arc<Ring>);

/// A queue holding up to `capacity` commands (rounded up to a power of two).
pub fn channel(capacity: usize) -> (Producer, Consumer) {
    let capacity = capacity.next_power_of_two().max(2);
    let ring = Arc::new(Ring {
        slots: (0..capacity).map(|_| AtomicU64::new(0)).collect(),
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
    });
    (Producer(ring.clone()), Consumer(ring))
}

impl Producer {
    /// Queue a command; false if the queue is full (the command is dropped).
    pub fn push(&self, command: Command) -> bool {
        let ring = &self.0;
        let tail = ring.tail.load(Ordering::Relaxed);
        let head = ring.head.load(Ordering::Acquire);
        if tail.wrapping_sub(head) == ring.slots.len() {
            return false;
        }
        ring.slots[tail & (ring.slots.len() - 1)].store(command.pack(), Ordering::Relaxed);
        ring.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }
}

impl Consumer {
    pub fn pop(&self) -> Option<Command> {
        let ring = &self.0;
        let head = ring.head.load(Ordering::Relaxed);
        let tail = ring.tail.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let word = ring.slots[head & (ring.slots.len() - 1)].load(Ordering::Relaxed);
        ring.head.store(head.wrapping_add(1), Ordering::Release);
        // A slot that fails to unpack can't happen; skip it rather than stall.
        Command::unpack(word).or_else(|| self.pop())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(n: u8) -> Command {
        Command::Play {
            sound: SoundId::Clash,
            gain: n,
            pan: -5,
            priority: 200,
        }
    }

    #[test]
    fn commands_survive_packing() {
        let all = [
            play(7),
            Command::Play {
                sound: SoundId::MusicBattle,
                gain: 255,
                pan: 127,
                priority: 0,
            },
            Command::MusicIntensity(9),
            Command::MusicVolume(0),
            Command::MasterVolume(255),
            Command::StopEffects,
        ];
        for c in all {
            assert_eq!(Command::unpack(c.pack()), Some(c));
        }
        assert_eq!(Command::unpack(0), None);
    }

    #[test]
    fn fifo_and_full() {
        let (tx, rx) = channel(4);
        assert!(rx.pop().is_none());
        for n in 0..4 {
            assert!(tx.push(play(n)));
        }
        assert!(!tx.push(play(99)), "queue should be full");
        for n in 0..4 {
            assert_eq!(rx.pop(), Some(play(n)));
        }
        assert!(rx.pop().is_none());
        assert!(tx.push(play(5)));
        assert_eq!(rx.pop(), Some(play(5)));
    }

    #[test]
    fn across_threads_nothing_lost_or_reordered() {
        let (tx, rx) = channel(64);
        let sender = std::thread::spawn(move || {
            for n in 0..10_000u32 {
                let c = Command::MusicIntensity((n % 251) as u8);
                while !tx.push(c) {
                    std::thread::yield_now();
                }
            }
        });
        let mut got = 0u32;
        while got < 10_000 {
            if let Some(c) = rx.pop() {
                assert_eq!(c, Command::MusicIntensity((got % 251) as u8));
                got += 1;
            } else {
                std::thread::yield_now();
            }
        }
        sender.join().unwrap();
    }
}
