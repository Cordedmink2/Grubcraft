//! Recorded voices: a hand-written WAV reader and the little toolkit that
//! turns a few recorded shouts into a regiment's battle cry (pitch them,
//! bend them, layer them with small offsets so many soldiers shout at
//! once, then filter and thicken the result).
//!
//! The recordings live under `assets-src/voices/` (sources and licences in
//! `assets-src/MANIFEST.md`). Only 16-bit PCM WAV is read; the
//! `.ogg`, `.flac` and `.mp3` downloads are converted once to mono 16-bit WAV
//! with ffmpeg and committed beside the originals.

use crate::synth::{Rng, SFX_RATE};
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::rc::Rc;

/// Where the recordings are: `assets-src/voices/` in the repository, found
/// from this crate's own directory so it works from any working directory
/// (the tests run in the crate's).
const VOICE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets-src/voices");

/// A decoded recording: mono samples in -1..1.
pub struct Wav {
    pub rate: u32,
    pub s: Vec<f32>,
}

/// Decode a 16-bit PCM WAV (mono, or stereo mixed down to mono).
pub fn parse_wav(bytes: &[u8]) -> Result<Wav, String> {
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let (mut at, mut format) = (12, None);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(at + 4) as usize;
        let body = at + 8;
        let end = (body + size).min(bytes.len());
        match id {
            b"fmt " if size >= 16 => {
                format = Some((
                    u16_at(body),
                    u16_at(body + 2),
                    u32_at(body + 4),
                    u16_at(body + 14),
                ));
            }
            b"data" => {
                let (tag, channels, rate, bits) = format.ok_or("data before fmt")?;
                // 1 is PCM; 0xFFFE is "extensible", which holds PCM here too.
                if !(tag == 1 || tag == 0xFFFE) || bits != 16 || !(1..=2).contains(&channels) {
                    return Err(format!(
                        "need 16-bit PCM mono or stereo (tag {tag}, {channels} channels, {bits} bits)"
                    ));
                }
                let channels = usize::from(channels);
                let s = bytes[body..end]
                    .chunks_exact(2 * channels)
                    .map(|f| {
                        let sum: f32 = f
                            .chunks_exact(2)
                            .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])))
                            .sum();
                        sum / channels as f32 / 32768.0
                    })
                    .collect();
                return Ok(Wav { rate, s });
            }
            _ => {}
        }
        at = body + size + (size & 1);
    }
    Err("no data chunk".into())
}

thread_local! {
    static CACHE: RefCell<HashMap<&'static str, Rc<Wav>>> = RefCell::new(HashMap::new());
}

/// The recording `file` (a path under `assets-src/voices/`), read once.
fn load(file: &'static str) -> Rc<Wav> {
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(file)
            .or_insert_with(|| {
                let path = format!("{VOICE_DIR}/{file}");
                let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
                Rc::new(parse_wav(&bytes).unwrap_or_else(|e| panic!("{path}: {e}")))
            })
            .clone()
    })
}

/// One recorded shout placed in a layer: which part of which file, when it
/// starts, how far it is pitched and bent, and how loud.
#[derive(Clone, Copy)]
pub struct Take {
    pub file: &'static str,
    /// Start and length within the recording, in seconds (0 length: to the end).
    pub from: f32,
    pub len: f32,
    /// Seconds into the layer when it starts.
    pub at: f32,
    /// Pitch change in semitones (speed changes with it, as with a tape).
    pub semi: f32,
    /// Further semitones gained (or lost, if negative) over the take.
    pub glide: f32,
    pub gain: f32,
    /// Seconds to sustain a steady recording for, looping it with a
    /// crossfade when it is shorter than that (0: play it once).
    pub hold: f32,
}

impl Take {
    pub const fn new(file: &'static str, at: f32, semi: f32, gain: f32) -> Take {
        Take {
            file,
            from: 0.0,
            len: 0.0,
            at,
            semi,
            glide: 0.0,
            gain,
            hold: 0.0,
        }
    }

    /// Use only `len` seconds of the recording from `from`.
    pub const fn cut(mut self, from: f32, len: f32) -> Take {
        self.from = from;
        self.len = len;
        self
    }

    /// Sustain for `seconds` by looping the cut (for steady sung notes).
    pub const fn hold(mut self, seconds: f32) -> Take {
        self.hold = seconds;
        self
    }

    /// Bend the pitch by `semitones` over the take.
    pub const fn glide(mut self, semitones: f32) -> Take {
        self.glide = semitones;
        self
    }
}

/// `n` copies of a take with small, repeatable differences in start, pitch
/// and loudness, like `n` soldiers shouting the same thing: the first is
/// the take itself, the rest follow within `spread` seconds and
/// `detune` semitones.
pub fn crowd(take: Take, n: u32, spread: f32, detune: f32, seed: u32) -> Vec<Take> {
    let mut rng = Rng(seed.wrapping_mul(2_654_435_761).max(1));
    (0..n)
        .map(|k| {
            if k == 0 {
                return take;
            }
            let (a, b, c) = (rng.noise(), rng.noise(), rng.noise());
            Take {
                at: (take.at + (a * 0.5 + 0.5) * spread).max(0.0),
                semi: take.semi + b * detune,
                gain: take.gain * (0.55 + 0.25 * (c * 0.5 + 0.5)),
                ..take
            }
        })
        .collect()
}

/// A layer being built: samples at [`SFX_RATE`].
pub struct Layer {
    pub s: Vec<f32>,
}

impl Layer {
    pub fn new(seconds: f32) -> Layer {
        Layer {
            s: vec![0.0; (seconds * SFX_RATE as f32).round() as usize],
        }
    }

    /// Mix in each take.
    pub fn lay(mut self, takes: &[Take]) -> Layer {
        for t in takes {
            self.add_take(t);
        }
        self
    }

    fn add_take(&mut self, t: &Take) {
        let wav = load(t.file);
        let start = (t.at * SFX_RATE as f32).round() as usize;
        for (v, x) in self.s.iter_mut().skip(start).zip(render(&wav, t)) {
            *v += x;
        }
    }

    /// Mix in `dur` seconds of white noise from `at`, fading in and out
    /// (filter the layer afterwards to make it a hiss).
    pub fn noise(mut self, at: f32, dur: f32, amp: f32, seed: u32) -> Layer {
        let rate = SFX_RATE as f32;
        let mut rng = Rng(seed.max(1));
        let first = (at * rate) as usize;
        let n = (dur * rate) as usize;
        for k in 0..n {
            let Some(v) = self.s.get_mut(first + k) else {
                break;
            };
            let x = k as f32 / n as f32;
            *v += amp * rng.noise() * (PI * x).sin().powi(2);
        }
        self
    }

    pub fn low(mut self, cutoff: f32) -> Layer {
        self.filter(Kind::Low, cutoff, 0.707);
        self
    }

    pub fn high(mut self, cutoff: f32) -> Layer {
        self.filter(Kind::High, cutoff, 0.707);
        self
    }

    /// Keep `lo..hi` Hz: a high-pass and a low-pass.
    pub fn band(self, lo: f32, hi: f32) -> Layer {
        self.high(lo).low(hi)
    }

    fn filter(&mut self, kind: Kind, cutoff: f32, q: f32) {
        let mut f = Biquad::new(kind, cutoff, q, SFX_RATE as f32);
        for v in &mut self.s {
            *v = f.run(*v);
        }
    }

    /// Chatter: chop the layer at `hz` (0 depth: untouched, 1: fully off
    /// between pulses), the clicking of mandibles.
    pub fn chatter(mut self, hz: f32, depth: f32) -> Layer {
        for (i, v) in self.s.iter_mut().enumerate() {
            let t = i as f32 / SFX_RATE as f32;
            // A squared sine: pulses with soft edges.
            let pulse = (0.5 + 0.5 * (TAU * hz * t).sin()).powi(2);
            *v *= 1.0 - depth + depth * pulse;
        }
        self
    }

    /// Thicken: a few quick, fading echoes, the sound of a crowd and the
    /// ground around it.
    pub fn bloom(mut self, amount: f32) -> Layer {
        let dry = self.s.clone();
        for (ms, g) in [
            (19.0, 0.55),
            (31.0, 0.42),
            (47.0, 0.33),
            (71.0, 0.24),
            (107.0, 0.16),
        ] {
            let d = (ms * 0.001 * SFX_RATE as f32) as usize;
            for i in d..self.s.len() {
                self.s[i] += dry[i - d] * g * amount;
            }
        }
        self
    }

    /// Shape the loudness over the layer: rise over `attack` seconds, fall
    /// over the last `release`.
    pub fn shape(mut self, attack: f32, release: f32) -> Layer {
        let n = self.s.len() as f32;
        let rate = SFX_RATE as f32;
        for (i, v) in self.s.iter_mut().enumerate() {
            let t = i as f32 / rate;
            let left = (n - i as f32) / rate;
            let a = (t / attack.max(1e-4)).min(1.0);
            let r = (left / release.max(1e-4)).min(1.0);
            *v *= a * a * (3.0 - 2.0 * a) * r;
        }
        self
    }

    /// Soft-clip to even out the loudest shouts (they stack).
    pub fn squash(mut self, drive: f32) -> Layer {
        for v in &mut self.s {
            *v = (*v * drive).tanh() / drive.tanh();
        }
        self
    }

    /// Mix another layer in.
    pub fn plus(mut self, other: &Layer) -> Layer {
        for (a, b) in self.s.iter_mut().zip(&other.s) {
            *a += b;
        }
        self
    }
}

/// The take as samples at [`SFX_RATE`]: cut, pitched, bent, faded at the
/// ends and scaled by its gain.
fn render(wav: &Wav, t: &Take) -> Vec<f32> {
    let src_rate = wav.rate as f32;
    let first = (t.from * src_rate) as usize;
    let last = if t.len > 0.0 {
        (((t.from + t.len) * src_rate) as usize).min(wav.s.len())
    } else {
        wav.s.len()
    };
    assert!(
        first + 4 < last,
        "{}: nothing to play from {}s",
        t.file,
        t.from
    );
    // Filter out what would fold back when played faster than it was
    // recorded, at the highest speed of the take.
    let fastest = ratio(t.semi + t.glide.max(0.0)) * src_rate / SFX_RATE as f32;
    let cutoff = (0.45 * SFX_RATE as f32 / fastest.max(1.0)).min(0.45 * src_rate);
    let (mut lp1, mut lp2) = (
        Biquad::new(Kind::Low, cutoff, 0.707, src_rate),
        Biquad::new(Kind::Low, cutoff, 0.707, src_rate),
    );
    let src: Vec<f32> = wav.s[first..last]
        .iter()
        .map(|&v| lp2.run(lp1.run(v)))
        .collect();
    let src = if t.hold > 0.0 {
        let need = (t.hold * src_rate * ratio(t.semi + t.glide / 2.0)) as usize;
        sustain(src, need)
    } else {
        src
    };
    let n = src.len() as f32;
    // Fade the ends of the cut so it doesn't click.
    let fade_in = 0.004 * SFX_RATE as f32;
    let fade_out = 0.03 * SFX_RATE as f32;
    let mut out = Vec::with_capacity((n / fastest.max(0.1)) as usize + 1);
    let mut p = 0.0f32;
    while p < n - 2.0 {
        let step = ratio(t.semi + t.glide * p / n) * src_rate / SFX_RATE as f32;
        let remaining = (n - p) / step;
        let env = (out.len() as f32 / fade_in).min(1.0) * (remaining / fade_out).min(1.0);
        out.push(t.gain * env * cubic(&src, p));
        p += step;
    }
    out
}

/// Make `src` `len` samples long by playing it again and again, each pass
/// crossfaded into the last (it should be a steady tone, or the seam shows).
fn sustain(src: Vec<f32>, len: usize) -> Vec<f32> {
    if src.len() >= len {
        return src[..len.max(8)].to_vec();
    }
    let fade = (src.len() / 4).max(1);
    let mut out = src.clone();
    while out.len() < len {
        let at = out.len() - fade;
        for i in 0..fade {
            let k = i as f32 / fade as f32 * std::f32::consts::FRAC_PI_2;
            out[at + i] = out[at + i] * k.cos() + src[i] * k.sin();
        }
        out.extend_from_slice(&src[fade..]);
    }
    out.truncate(len);
    out
}

/// Catmull-Rom interpolation of `x` at fractional position `p`.
fn cubic(x: &[f32], p: f32) -> f32 {
    let i = p.floor() as usize;
    let f = p - i as f32;
    let at = |k: isize| x[(i as isize + k).clamp(0, x.len() as isize - 1) as usize];
    let (a, b, c, d) = (at(-1), at(0), at(1), at(2));
    b + 0.5 * f * (c - a + f * (2.0 * a - 5.0 * b + 4.0 * c - d + f * (3.0 * (b - c) + d - a)))
}

#[derive(Clone, Copy)]
enum Kind {
    Low,
    High,
}

/// A two-pole filter (RBJ cookbook), direct form I.
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    fn new(kind: Kind, cutoff: f32, q: f32, rate: f32) -> Biquad {
        let w = TAU * cutoff.clamp(10.0, rate * 0.49) / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let (b0, b1, b2) = match kind {
            Kind::Low => ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0),
            Kind::High => ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0),
        };
        Biquad::normalised([b0, b1, b2], [1.0 + alpha, -2.0 * cos, 1.0 - alpha])
    }

    fn normalised(b: [f32; 3], a: [f32; 3]) -> Biquad {
        Biquad {
            b: [b[0] / a[0], b[1] / a[0], b[2] / a[0]],
            a: [a[1] / a[0], a[2] / a[0]],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// A short click-and-ring, tuned by `freq`: a bone knocking on bone.
pub fn knock(layer: &mut Layer, at: f32, freq: f32, amp: f32, seed: u32) {
    let rate = SFX_RATE as f32;
    let mut rng = Rng(seed.max(1));
    let start = (at * rate) as usize;
    let mut lp = 0.0f32;
    for k in 0..(0.09 * rate) as usize {
        let Some(v) = layer.s.get_mut(start + k) else {
            break;
        };
        let t = k as f32 / rate;
        // A noise tick, then a short wooden ring at two partials.
        lp += 0.5 * (rng.noise() - lp);
        let tick = lp * (-t / 0.003).exp();
        let ring =
            ((TAU * freq * t).sin() + 0.6 * (TAU * freq * 2.3 * t).sin()) * (-t / 0.018).exp();
        *v += amp * (0.7 * tick + 0.5 * ring);
    }
}

/// A glassy ring: inharmonic partials with long, uneven decays, for the
/// Shard Choir's crystal.
pub fn glass(layer: &mut Layer, at: f32, freq: f32, amp: f32, ring: f32) {
    let rate = SFX_RATE as f32;
    let start = (at * rate) as usize;
    // Partial ratios of a struck glass or bell, and how fast each fades.
    const PARTIALS: [(f32, f32, f32); 5] = [
        (1.0, 1.0, 1.0),
        (2.76, 0.6, 0.7),
        (5.4, 0.35, 0.45),
        (8.93, 0.2, 0.3),
        (13.3, 0.1, 0.2),
    ];
    for k in 0..(ring * 3.0 * rate) as usize {
        let Some(v) = layer.s.get_mut(start + k) else {
            break;
        };
        let t = k as f32 / rate;
        let attack = (t / 0.002).min(1.0);
        let mut sum = 0.0;
        for (ratio, a, d) in PARTIALS {
            let f = freq * ratio;
            if f < rate * 0.45 {
                sum += a * (TAU * f * t + ratio).sin() * (-t / (ring * d)).exp();
            }
        }
        *v += amp * attack * sum;
    }
}

/// `semitones` as a frequency ratio.
pub fn ratio(semitones: f32) -> f32 {
    2f32.powf(semitones / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(channels: u16, frames: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = frames.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&44_100u32.to_le_bytes());
        b.extend_from_slice(&(44_100 * 2 * u32::from(channels)).to_le_bytes());
        b.extend_from_slice(&(2 * channels).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        // A chunk the reader must skip, with an odd size and its padding.
        b.extend_from_slice(b"LIST\x03\0\0\0abc\0");
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(&data);
        b
    }

    #[test]
    fn reads_mono_and_mixes_stereo_down() {
        let mono = parse_wav(&wav(1, &[0, 16384, -32768])).unwrap();
        assert_eq!(mono.rate, 44_100);
        assert_eq!(mono.s, vec![0.0, 0.5, -1.0]);
        let stereo = parse_wav(&wav(2, &[16384, 0, -16384, -16384])).unwrap();
        assert_eq!(stereo.s, vec![0.25, -0.5]);
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert!(parse_wav(b"RIFF....WAVE").is_err());
        assert!(parse_wav(b"not a wav file at all").is_err());
        let mut eight = wav(1, &[1, 2]);
        eight[34] = 8;
        assert!(parse_wav(&eight).is_err());
    }

    #[test]
    fn pitching_up_an_octave_halves_the_length_and_doubles_the_pitch() {
        let tone = Wav {
            rate: 44_100,
            s: (0..44_100)
                .map(|i| (TAU * 220.0 * i as f32 / 44_100.0).sin())
                .collect(),
        };
        let plain = render(&tone, &Take::new("tone", 0.0, 0.0, 1.0));
        let up = render(&tone, &Take::new("tone", 0.0, 12.0, 1.0));
        assert!((plain.len() as f32 - SFX_RATE as f32).abs() < 4.0);
        assert!((up.len() as f32 - SFX_RATE as f32 / 2.0).abs() < 4.0);
        let crossings = |v: &[f32]| v.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        // 220 Hz for one second, then 440 Hz for half a second: the same
        // number of cycles, in half the time.
        assert!(
            crossings(&plain).abs_diff(220) <= 2,
            "{}",
            crossings(&plain)
        );
        assert!(crossings(&up).abs_diff(220) <= 3, "{}", crossings(&up));
    }

    #[test]
    fn steady_takes_can_be_held() {
        let tone = Wav {
            rate: 22_050,
            s: (0..11_025)
                .map(|i| (TAU * 441.0 * i as f32 / 22_050.0).sin())
                .collect(),
        };
        let held = render(&tone, &Take::new("t", 0.0, 0.0, 1.0).hold(2.0));
        assert!((held.len() as f32 - 44_100.0).abs() < 8.0, "{}", held.len());
        // No hole where the passes join.
        let mid = &held[20_000..24_000];
        assert!(mid.iter().fold(0.0f32, |m, v| m.max(v.abs())) > 0.5);
    }

    #[test]
    fn takes_are_cut_and_scaled() {
        let tone = Wav {
            rate: 22_050,
            s: vec![1.0; 22_050],
        };
        let cut = render(&tone, &Take::new("t", 0.0, 0.0, 0.5).cut(0.25, 0.5));
        assert!((cut.len() as f32 - 11_025.0).abs() < 4.0);
        assert!((cut[cut.len() / 2] - 0.5).abs() < 0.02);
        assert!(cut[0].abs() < 0.01, "fades in");
    }

    #[test]
    fn crowds_are_repeatable_and_loosen_up() {
        let t = Take::new("x.wav", 0.1, 0.0, 1.0);
        let (a, b) = (crowd(t, 5, 0.2, 1.0, 7), crowd(t, 5, 0.2, 1.0, 7));
        assert_eq!(a.len(), 5);
        assert_eq!(a[0].at, 0.1);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((x.at, x.semi, x.gain), (y.at, y.semi, y.gain));
        }
        assert!(
            a[1..]
                .iter()
                .all(|x| x.at >= 0.1 && x.at <= 0.31 && x.gain < 1.0)
        );
    }
}
