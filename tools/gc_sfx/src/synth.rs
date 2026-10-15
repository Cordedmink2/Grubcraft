//! The sounds themselves: small additive/subtractive synthesis recipes.

use gc_audio::SoundId;
use std::f32::consts::TAU;

pub const SFX_RATE: u32 = 22_050;
pub const MUSIC_RATE: u32 = 16_000;

pub struct Rendered {
    pub id: SoundId,
    pub rate: u32,
    pub looped: bool,
    pub pcm: Vec<i16>,
}

/// Every clip in the bank, in [`SoundId`] order.
pub fn all() -> Vec<Rendered> {
    let sfx = |id, peak: f32, buf: Buf| Rendered {
        id,
        rate: SFX_RATE,
        looped: false,
        pcm: buf.finish(peak, true),
    };
    let music = |id, buf: Buf| Rendered {
        id,
        rate: MUSIC_RATE,
        looped: true,
        pcm: buf.finish(0.8, false),
    };
    vec![
        sfx(SoundId::ArrowRelease, 0.55, arrow_release()),
        sfx(SoundId::ArrowImpact, 0.7, arrow_impact()),
        sfx(SoundId::Clash, 0.85, clash()),
        sfx(SoundId::Charge, 0.9, charge()),
        sfx(SoundId::Waver, 0.6, waver()),
        sfx(SoundId::RoutHorn, 0.85, rout_horn()),
        sfx(SoundId::Shatter, 0.9, shatter()),
        sfx(SoundId::Rally, 0.8, rally()),
        sfx(SoundId::UiSelect, 0.45, ui_select()),
        sfx(SoundId::UiMove, 0.45, ui_move()),
        sfx(SoundId::UiAttack, 0.5, ui_attack()),
        music(SoundId::MusicCalm, music_calm()),
        music(SoundId::MusicBattle, music_battle()),
    ]
}

// ---------------------------------------------------------------- toolkit

/// A mono float buffer being composed.
struct Buf {
    rate: f32,
    s: Vec<f32>,
    /// Sounds written near the end wrap around to the start (for loops).
    wrap: bool,
}

impl Buf {
    fn new(seconds: f32, rate: u32) -> Buf {
        Buf {
            rate: rate as f32,
            s: vec![0.0; (seconds * rate as f32).round() as usize],
            wrap: false,
        }
    }

    fn looping(seconds: f32, rate: u32) -> Buf {
        Buf {
            wrap: true,
            ..Buf::new(seconds, rate)
        }
    }

    fn at(&mut self, start: f32, mut f: impl FnMut(f32, &mut Rng) -> f32, dur: f32, seed: u32) {
        let first = (start * self.rate).round() as usize;
        let n = (dur * self.rate) as usize;
        let mut rng = Rng(seed);
        for k in 0..n {
            let idx = first + k;
            let idx = if self.wrap {
                idx % self.s.len()
            } else if idx < self.s.len() {
                idx
            } else {
                break;
            };
            self.s[idx] += f(k as f32 / self.rate, &mut rng);
        }
    }

    /// Normalise to `peak`, fade the last few milliseconds, quantise.
    fn finish(mut self, peak: f32, fade: bool) -> Vec<i16> {
        let max = self.s.iter().fold(1e-6f32, |m, v| m.max(v.abs()));
        let n = self.s.len();
        let fade_len = (self.rate * 0.012) as usize;
        for (i, v) in self.s.iter_mut().enumerate() {
            *v *= peak / max;
            if fade && i + fade_len > n {
                *v *= (n - i) as f32 / fade_len as f32;
            }
        }
        self.s.iter().map(|v| (v * 32767.0) as i16).collect()
    }
}

/// A tiny xorshift noise source; every sound seeds its own so the output
/// never depends on render order.
struct Rng(u32);

impl Rng {
    fn noise(&mut self) -> f32 {
        let mut x = self.0.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn decay(t: f32, tau: f32) -> f32 {
    (-t / tau).exp()
}

/// Rise over `attack` seconds, hold, fall over `release` seconds before `dur`.
fn adsr(t: f32, attack: f32, release: f32, dur: f32) -> f32 {
    let a = (t / attack).min(1.0);
    let r = ((dur - t) / release).clamp(0.0, 1.0);
    a * a * (3.0 - 2.0 * a) * r
}

/// One-pole filter state, usable as low-pass or high-pass.
#[derive(Default)]
struct OnePole {
    y: f32,
}

impl OnePole {
    fn low(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
        let a = 1.0 - (-TAU * cutoff / rate).exp();
        self.y += a * (x - self.y);
        self.y
    }

    fn high(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
        x - self.low(x, cutoff, rate)
    }
}

/// Band-limited-ish sum of harmonics with a 1/k^power rolloff.
fn harmonics(phase: f32, count: u32, power: f32) -> f32 {
    (1..=count)
        .map(|k| (phase * k as f32).sin() / (k as f32).powf(power))
        .sum()
}

/// A drum hit: pitched thump with a short noise attack.
fn drum(buf: &mut Buf, at: f32, amp: f32, base: f32, seed: u32) {
    let rate = buf.rate;
    let mut phase = 0.0f32;
    let mut lp = OnePole::default();
    buf.at(
        at,
        |t, rng| {
            let f = base * (1.0 + 1.2 * decay(t, 0.05));
            phase += TAU * f / rate;
            let body = phase.sin() * decay(t, 0.22);
            let skin = lp.low(rng.noise(), 500.0, rate) * decay(t, 0.03) * 1.5;
            amp * (body + skin)
        },
        0.9,
        seed,
    );
}

/// Karplus-Strong plucked string.
fn pluck(buf: &mut Buf, at: f32, freq: f32, amp: f32, damping: f32, seed: u32) {
    let len = (buf.rate / freq).round().max(2.0) as usize;
    let mut rng = Rng(seed);
    let mut line: Vec<f32> = (0..len).map(|_| rng.noise()).collect();
    let mut i = 0;
    let dur = 1.2;
    buf.at(
        at,
        |_, _| {
            let next = line[(i + 1) % len];
            let v = line[i];
            line[i] = damping * 0.5 * (v + next);
            i = (i + 1) % len;
            v * amp
        },
        dur,
        seed,
    );
}

/// A brassy horn note with a little vibrato and breath.
fn horn(buf: &mut Buf, at: f32, freq: f32, dur: f32, amp: f32, seed: u32) {
    let rate = buf.rate;
    let mut phase = 0.0f32;
    let mut lp = OnePole::default();
    buf.at(
        at,
        |t, rng| {
            // Notes start a touch flat and slide up, like a real lip.
            let slide = 1.0 - 0.04 * decay(t, 0.05);
            let vib = 1.0 + 0.006 * (TAU * 5.2 * t).sin() * (t * 4.0).min(1.0);
            phase += TAU * freq * slide * vib / rate;
            let tone = harmonics(phase, 9, 1.1);
            let breath = lp.low(rng.noise(), 2500.0, rate) * 0.12 * decay(t, 0.15);
            amp * (tone * 0.55 + breath) * adsr(t, 0.07, 0.25, dur)
        },
        dur,
        seed,
    );
}

// ---------------------------------------------------------------- effects

fn arrow_release() -> Buf {
    let mut b = Buf::new(0.55, SFX_RATE);
    let rate = b.rate;
    // The string: a quick, slightly sagging twang.
    let mut phase = 0.0;
    b.at(
        0.0,
        |t, _| {
            let f = 196.0 * (1.0 + 0.06 * decay(t, 0.03));
            phase += TAU * f / rate;
            (phase.sin() + 0.5 * (phase * 2.0).sin() + 0.25 * (phase * 3.0).sin()) * decay(t, 0.09)
        },
        0.5,
        1,
    );
    // The arrows rushing away: noise sweeping upward.
    let (mut hp, mut lp) = (OnePole::default(), OnePole::default());
    b.at(
        0.01,
        |t, rng| {
            let cutoff = 700.0 + 5000.0 * (t / 0.3).min(1.0);
            let n = lp.low(hp.high(rng.noise(), 600.0, rate), cutoff, rate);
            n * 0.7 * (t / 0.02).min(1.0) * decay(t, 0.09)
        },
        0.4,
        2,
    );
    b
}

fn arrow_impact() -> Buf {
    let mut b = Buf::new(0.35, SFX_RATE);
    let rate = b.rate;
    // A volley lands as a ragged handful of thunks.
    for (i, (at, amp, f)) in [
        (0.0, 1.0, 150.0),
        (0.05, 0.7, 120.0),
        (0.11, 0.8, 175.0),
        (0.16, 0.5, 130.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut phase = 0.0;
        let mut lp = OnePole::default();
        b.at(
            at,
            |t, rng| {
                phase += TAU * f * (1.0 + decay(t, 0.02)) / rate;
                let thump = phase.sin() * decay(t, 0.045);
                let crack = lp.low(rng.noise(), 2200.0, rate) * decay(t, 0.012) * 0.9;
                amp * (thump + crack)
            },
            0.2,
            10 + i as u32,
        );
    }
    b
}

fn clash() -> Buf {
    let mut b = Buf::new(0.7, SFX_RATE);
    let rate = b.rate;
    let mut hp = OnePole::default();
    // The strike.
    b.at(
        0.0,
        |t, rng| hp.high(rng.noise(), 1500.0, rate) * decay(t, 0.012) * 1.2,
        0.1,
        3,
    );
    // The ring: inharmonic partials of struck steel.
    let partials = [
        (1.0, 0.7, 0.30),
        (2.76, 0.55, 0.20),
        (5.4, 0.35, 0.12),
        (8.93, 0.2, 0.07),
    ];
    for (ratio, amp, tau) in partials {
        let f = 640.0 * ratio;
        b.at(
            0.0,
            |t, _| (TAU * f * t).sin() * amp * decay(t, tau),
            0.7,
            4,
        );
    }
    // Body behind it.
    b.at(
        0.0,
        |t, _| (TAU * 95.0 * t).sin() * decay(t, 0.05) * 0.5,
        0.2,
        5,
    );
    b
}

fn charge() -> Buf {
    let mut b = Buf::new(1.4, SFX_RATE);
    // A drum roll that speeds up and swells into a final heavy beat.
    for (i, (at, amp)) in [
        (0.0, 0.55),
        (0.30, 0.6),
        (0.52, 0.65),
        (0.68, 0.7),
        (0.80, 0.75),
        (0.90, 0.8),
        (0.98, 0.9),
        (1.06, 1.0),
    ]
    .into_iter()
    .enumerate()
    {
        drum(&mut b, at, amp, 82.0, 20 + i as u32);
    }
    b
}

fn waver() -> Buf {
    let mut b = Buf::new(1.0, SFX_RATE);
    let dur = 1.0;
    for (f, amp) in [(98.0, 1.0), (103.5, 0.9), (147.0, 0.35)] {
        b.at(
            0.0,
            |t, _| {
                let sink = 1.0 - 0.06 * t;
                let tremolo = 0.65 + 0.35 * (TAU * 5.5 * t).sin();
                (TAU * f * sink * t).sin() * amp * tremolo * adsr(t, 0.2, 0.5, dur)
            },
            dur,
            6,
        );
    }
    b
}

fn rout_horn() -> Buf {
    let mut b = Buf::new(1.25, SFX_RATE);
    // Short, then long: the signal for a broken line.
    horn(&mut b, 0.0, 233.1, 0.34, 1.0, 7);
    horn(&mut b, 0.42, 233.1, 0.8, 1.0, 8);
    b
}

fn shatter() -> Buf {
    let mut b = Buf::new(1.6, SFX_RATE);
    let rate = b.rate;
    let mut lp = OnePole::default();
    // A crash of noise whose brightness falls away.
    b.at(
        0.0,
        |t, rng| {
            let cutoff = 300.0 + 6500.0 * decay(t, 0.25);
            lp.low(rng.noise(), cutoff, rate) * decay(t, 0.45) * 1.3
        },
        1.5,
        9,
    );
    // Falling bits of metal.
    let mut rng = Rng(77);
    for i in 0..9 {
        let f = 450.0 + (rng.noise() * 0.5 + 0.5) * 2800.0;
        let start = 0.02 + i as f32 * 0.055 + (rng.noise() * 0.5 + 0.5) * 0.03;
        let tau = 0.12 + (rng.noise() * 0.5 + 0.5) * 0.35;
        b.at(
            start,
            |t, _| (TAU * f * t).sin() * decay(t, tau) * 0.22,
            1.0,
            30 + i,
        );
    }
    // The boom under it.
    b.at(
        0.0,
        |t, _| (TAU * 52.0 * (1.0 + 0.5 * decay(t, 0.1)) * t).sin() * decay(t, 0.3) * 1.1,
        0.9,
        11,
    );
    b
}

fn rally() -> Buf {
    let mut b = Buf::new(1.15, SFX_RATE);
    // Two rising notes: a fifth up, held.
    horn(&mut b, 0.0, 196.0, 0.36, 0.9, 12);
    horn(&mut b, 0.4, 293.7, 0.75, 1.0, 13);
    b
}

fn ui_select() -> Buf {
    let mut b = Buf::new(0.1, SFX_RATE);
    for (f, amp) in [(1180.0, 1.0), (1770.0, 0.4)] {
        b.at(
            0.0,
            |t, _| (TAU * f * t).sin() * amp * decay(t, 0.025),
            0.1,
            14,
        );
    }
    b
}

fn ui_move() -> Buf {
    let mut b = Buf::new(0.18, SFX_RATE);
    for (at, f) in [(0.0, 660.0), (0.075, 880.0)] {
        b.at(
            at,
            |t, _| {
                let tone = (TAU * f * t).sin() + 0.3 * (TAU * f * 2.0 * t).sin();
                tone * decay(t, 0.03) * (t / 0.004).min(1.0)
            },
            0.1,
            15,
        );
    }
    b
}

fn ui_attack() -> Buf {
    let mut b = Buf::new(0.24, SFX_RATE);
    let rate = b.rate;
    let mut phase = 0.0;
    b.at(
        0.0,
        |t, _| {
            let f = 220.0 + 330.0 * (t / 0.14).min(1.0);
            phase += TAU * f / rate;
            harmonics(phase, 6, 1.0) * decay(t, 0.1) * 0.7
        },
        0.24,
        16,
    );
    let mut hp = OnePole::default();
    b.at(
        0.0,
        |t, rng| hp.high(rng.noise(), 1800.0, rate) * decay(t, 0.015),
        0.05,
        17,
    );
    b
}

// ------------------------------------------------------------------ music

const BPM: f32 = 100.0;
const BEAT: f32 = 60.0 / BPM;
const BEATS: f32 = 32.0; // eight bars of four
const LOOP_SECONDS: f32 = BEAT * BEATS;

/// Hirajoshi in D, the Clans' scale (Hz).
mod note {
    pub const D2: f32 = 73.42;
    pub const A2: f32 = 110.0;
    pub const EB3: f32 = 155.56;
    pub const D3: f32 = 146.83;
    pub const A3: f32 = 220.0;
    pub const D4: f32 = 293.66;
    pub const EB4: f32 = 311.13;
    pub const G4: f32 = 392.0;
    pub const A4: f32 = 440.0;
    pub const BB4: f32 = 466.16;
}

/// A breathy flute note, shakuhachi-style.
fn flute(b: &mut Buf, beat: f32, beats: f32, freq: f32, amp: f32, seed: u32) {
    let rate = b.rate;
    let dur = beats * BEAT;
    let mut phase = 0.0f32;
    let mut lp = OnePole::default();
    b.at(
        beat * BEAT,
        |t, rng| {
            let vib = 1.0 + 0.005 * (TAU * 5.0 * t).sin() * ((t - 0.25) * 3.0).clamp(0.0, 1.0);
            phase += TAU * freq * vib / rate;
            let tone = phase.sin() + 0.18 * (phase * 2.0).sin() + 0.06 * (phase * 3.0).sin();
            let breath = lp.low(rng.noise(), freq * 3.0, rate) * 0.35;
            amp * (tone + breath) * adsr(t, 0.14, 0.35, dur)
        },
        dur,
        seed,
    );
}

/// A sustained low pad: a few harmonics with a slow swell.
fn pad(b: &mut Buf, beat: f32, beats: f32, freq: f32, amp: f32) {
    let rate = b.rate;
    let dur = beats * BEAT;
    let mut phase = 0.0f32;
    b.at(
        beat * BEAT,
        |t, _| {
            phase += TAU * freq / rate;
            harmonics(phase, 5, 1.4) * amp * adsr(t, dur * 0.4, dur * 0.4, dur)
        },
        dur,
        0,
    );
}

fn music_calm() -> Buf {
    let mut b = Buf::looping(LOOP_SECONDS, MUSIC_RATE);
    // Drone: root and fifth under everything, swelling over four bars.
    for bar in [0.0, 16.0] {
        pad(&mut b, bar, 16.0, note::D2, 0.45);
        pad(&mut b, bar, 16.0, note::A2, 0.30);
        pad(&mut b, bar, 16.0, note::D3, 0.12);
    }
    // The melody, in beats from the start of the loop.
    let melody = [
        (0.0, 3.0, note::D4),
        (4.0, 2.0, note::G4),
        (6.0, 2.0, note::A4),
        (8.0, 3.0, note::BB4),
        (11.0, 1.0, note::A4),
        (12.0, 4.0, note::G4),
        (16.0, 3.0, note::A4),
        (19.0, 1.0, note::G4),
        (20.0, 2.0, note::EB4),
        (22.0, 2.0, note::D4),
        (24.0, 6.0, note::D4),
    ];
    for (i, (beat, len, f)) in melody.into_iter().enumerate() {
        flute(&mut b, beat, len, f, 0.55, 100 + i as u32);
    }
    // A string answering at the end of each phrase.
    for (i, beat) in [3.5, 11.5, 19.5, 27.0].into_iter().enumerate() {
        pluck(&mut b, beat * BEAT, note::D3, 0.35, 0.997, 200 + i as u32);
    }
    // Sparse drum: the downbeat of every second bar.
    for (i, beat) in [0.0, 8.0, 16.0, 24.0].into_iter().enumerate() {
        drum(&mut b, beat * BEAT, 0.5, 65.0, 300 + i as u32);
    }
    b
}

fn music_battle() -> Buf {
    let mut b = Buf::looping(LOOP_SECONDS, MUSIC_RATE);
    // Drums: a pulse on every beat, hard on the bar, with fills at the end
    // of each four-bar phrase.
    for beat in 0..32 {
        let accent = if beat % 4 == 0 { 1.0 } else { 0.55 };
        drum(&mut b, beat as f32 * BEAT, accent * 0.9, 72.0, 400 + beat);
    }
    for (i, start) in [14.0f32, 30.0].into_iter().enumerate() {
        for k in 0..4 {
            drum(
                &mut b,
                (start + 0.25 * k as f32 * 2.0) * BEAT,
                0.6 + 0.1 * k as f32,
                95.0,
                500 + (i * 4 + k) as u32,
            );
        }
        drum(&mut b, (start + 1.5) * BEAT, 0.9, 80.0, 520 + i as u32);
    }
    // A driving plucked ostinato, in eighth notes.
    let pattern = [
        note::D3,
        0.0,
        note::D3,
        note::EB3,
        note::D3,
        0.0,
        note::A2,
        note::EB3,
    ];
    for step in 0..64 {
        let f = pattern[step % 8];
        if f > 0.0 {
            pluck(
                &mut b,
                step as f32 * 0.5 * BEAT,
                f,
                0.5,
                0.995,
                600 + step as u32,
            );
        }
    }
    // Brass pad holding the root and fifth, swelling every four bars.
    for bar in [0.0, 16.0] {
        pad(&mut b, bar, 16.0, note::D3, 0.5);
        pad(&mut b, bar, 16.0, note::A3, 0.3);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_present_in_order() {
        let clips = all();
        assert_eq!(clips.len(), SoundId::COUNT);
        for (clip, id) in clips.iter().zip(SoundId::ALL) {
            assert_eq!(clip.id, id);
        }
    }

    #[test]
    fn sounds_are_audible_not_clipped_and_sized_sensibly() {
        for c in all() {
            let peak = c.pcm.iter().map(|s| s.unsigned_abs()).max().unwrap();
            assert!(peak > 8000, "{:?} is too quiet", c.id);
            assert!(peak < 32767, "{:?} clips", c.id);
            let seconds = c.pcm.len() as f32 / c.rate as f32;
            if c.id.is_music() {
                assert!(c.looped);
                assert!((seconds - LOOP_SECONDS).abs() < 0.01, "{:?}", c.id);
            } else {
                assert!(!c.looped);
                assert!((0.05..2.0).contains(&seconds), "{:?} is {seconds}s", c.id);
            }
        }
    }

    #[test]
    fn music_layers_are_the_same_length() {
        let clips = all();
        assert_eq!(
            clips[SoundId::MusicCalm as usize].pcm.len(),
            clips[SoundId::MusicBattle as usize].pcm.len()
        );
    }

    #[test]
    fn loops_join_without_a_click() {
        for c in all().into_iter().filter(|c| c.looped) {
            let last = i32::from(*c.pcm.last().unwrap());
            let first = i32::from(c.pcm[0]);
            // Neighbouring samples in the middle of the music differ by this much.
            let typical = c
                .pcm
                .windows(2)
                .map(|w| (i32::from(w[1]) - i32::from(w[0])).abs())
                .sum::<i32>()
                / c.pcm.len() as i32;
            assert!(
                (first - last).abs() < typical * 12 + 1500,
                "{:?} jumps {} at the loop point (typical step {typical})",
                c.id,
                (first - last).abs()
            );
        }
    }
}
