//! The mixer: up to 32 sound-effect voices, four ambient beds and two music layers, mixed to
//! interleaved stereo `f32`. It runs on the audio thread and never allocates
//! or locks: all its memory is sized in [`Mixer::new`], and the main thread
//! talks to it only through the command queue.

use crate::adpcm::Decoder;
use crate::bank::{Bank, Clip, SoundId};
use crate::queue::{Command, Consumer};
use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Sound-effect voices playing at once.
pub const MAX_VOICES: usize = 32;
/// No single sound may take more voices than this, so a big melee can't push
/// every other sound out.
const MAX_PER_SOUND: usize = 8;
/// Headroom for effects: many voices can add up.
const EFFECT_LEVEL: f32 = 0.35;
const MUSIC_LEVEL: f32 = 0.5;
/// Headroom for the ambient beds, which sit under everything else.
const BED_LEVEL: f32 = 0.45;
/// A bed's level, pan and muffling move this fraction of the way to their
/// targets per second.
const BED_RATE: f32 = 1.5;
/// The low-pass cutoff (Hz) for a fully muffled sound (`dull` 255); `dull`
/// 0 is unfiltered, and the cutoff falls smoothly between.
const MUFFLED_CUTOFF: f32 = 450.0;
const CLEAR_CUTOFF: f32 = 16_000.0;
/// Intensity (0..1) moves this fraction of the way to its target per second.
const INTENSITY_RATE: f32 = 0.6;

/// Counters the main thread can read (for the dev overlay).
#[derive(Default)]
pub struct Meter {
    /// Effect voices playing now.
    pub active: AtomicU32,
    /// Sounds that found no free voice and were dropped.
    pub dropped: AtomicU32,
}

/// Position in a clip, with enough state to decode and resample on the fly.
#[derive(Clone, Copy)]
struct Cursor {
    decoder: Decoder,
    next: u32,
    s0: f32,
    s1: f32,
    phase: f32,
    ending: bool,
    finished: bool,
}

impl Cursor {
    fn start(clip: &Clip) -> Cursor {
        let mut c = Cursor {
            decoder: Decoder::new(),
            next: 0,
            s0: 0.0,
            s1: 0.0,
            phase: 0.0,
            ending: false,
            finished: false,
        };
        c.s0 = c.pull(clip).unwrap_or(0.0);
        c.s1 = c.pull(clip).unwrap_or(0.0);
        c
    }

    /// Decode the next stored sample, wrapping if the clip loops.
    fn pull(&mut self, clip: &Clip) -> Option<f32> {
        if self.next >= clip.samples {
            if !clip.looped {
                return None;
            }
            self.decoder = Decoder::new();
            self.next = 0;
        }
        let byte = clip.data[(self.next / 2) as usize];
        let nibble = if self.next.is_multiple_of(2) {
            byte & 15
        } else {
            byte >> 4
        };
        self.next += 1;
        Some(f32::from(self.decoder.decode(nibble)) * (1.0 / 32768.0))
    }

    /// The next output sample, `step` source samples further on (linear
    /// interpolation). None once the clip has played out.
    fn sample(&mut self, clip: &Clip, step: f32) -> Option<f32> {
        if self.finished {
            return None;
        }
        let out = self.s0 + (self.s1 - self.s0) * self.phase;
        self.phase += step;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.s0 = self.s1;
            match self.pull(clip) {
                Some(v) => self.s1 = v,
                None if self.ending => {
                    self.finished = true;
                    break;
                }
                None => {
                    self.ending = true;
                    self.s1 = 0.0;
                }
            }
        }
        Some(out)
    }
}

#[derive(Clone, Copy)]
struct Voice {
    sound: SoundId,
    cursor: Cursor,
    step: f32,
    left: f32,
    right: f32,
    /// Priority then gain: who gets stolen from first.
    score: u32,
    /// Start order, to find the oldest.
    age: u32,
    active: bool,
    /// Low-pass coefficient (1.0 = off) and its state: distance takes the
    /// highs off a sound.
    alpha: f32,
    lp: f32,
}

/// Playback speed for a pitch byte: 128 is as recorded, 0 an octave down,
/// 255 about an octave up.
fn pitch_ratio(pitch: u8) -> f32 {
    ((f32::from(pitch) - 128.0) / 128.0).exp2()
}

/// Left and right gains for a pan of -1 (left) to 1 (right).
fn pan_gains(pan: f32) -> (f32, f32) {
    ((1.0 - pan).min(1.0), (1.0 + pan).min(1.0))
}

/// One-pole low-pass coefficient for a `dull` byte at output rate `rate`.
fn dull_alpha(dull: f32, rate: u32) -> f32 {
    if dull <= 0.0 {
        return 1.0;
    }
    let cutoff = CLEAR_CUTOFF * (MUFFLED_CUTOFF / CLEAR_CUTOFF).powf(dull.min(1.0));
    1.0 - (-TAU * cutoff / rate as f32).exp()
}

/// A looping ambient clip with a level, a place and a muffling that glide
/// towards what the game last asked for.
struct BedLayer {
    clip: Clip<'static>,
    cursor: Cursor,
    step: f32,
    level: f32,
    pan: f32,
    dull: f32,
    target: (f32, f32, f32),
    lp: f32,
}

impl BedLayer {
    fn new(clip: &Clip<'static>, rate: u32) -> BedLayer {
        BedLayer {
            clip: *clip,
            cursor: Cursor::start(clip),
            step: clip.rate as f32 / rate as f32,
            level: 0.0,
            pan: 0.0,
            dull: 0.0,
            target: (0.0, 0.0, 0.0),
            lp: 0.0,
        }
    }

    fn silent(&self) -> bool {
        self.level < 1e-4 && self.target.0 < 1e-4
    }
}

struct MusicLayer {
    clip: Clip<'static>,
    cursor: Cursor,
    step: f32,
}

pub struct Mixer {
    bank: Bank<'static>,
    rate: u32,
    voices: [Voice; MAX_VOICES],
    beds: [Option<BedLayer>; SoundId::BEDS.len()],
    music: Option<[MusicLayer; 2]>,
    music_volume: f32,
    intensity: f32,
    intensity_target: f32,
    master: f32,
    age: u32,
    commands: Consumer,
    meter: Arc<Meter>,
}

impl Mixer {
    /// `rate` is the output device's sample rate.
    pub fn new(bank: Bank<'static>, rate: u32, commands: Consumer, meter: Arc<Meter>) -> Mixer {
        let silent = Voice {
            sound: SoundId::UiSelect,
            cursor: Cursor {
                decoder: Decoder::new(),
                next: 0,
                s0: 0.0,
                s1: 0.0,
                phase: 0.0,
                ending: false,
                finished: true,
            },
            step: 0.0,
            left: 0.0,
            right: 0.0,
            score: 0,
            age: 0,
            active: false,
            alpha: 1.0,
            lp: 0.0,
        };
        let beds = SoundId::BEDS.map(|id| bank.clip(id).map(|clip| BedLayer::new(clip, rate)));
        Mixer {
            bank,
            rate,
            voices: [silent; MAX_VOICES],
            beds,
            music: None,
            music_volume: 0.0,
            intensity: 0.0,
            intensity_target: 0.0,
            master: 1.0,
            age: 0,
            commands,
            meter,
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Play {
                sound,
                gain,
                pan,
                priority,
                dull,
                pitch,
            } => self.play(sound, gain, pan, priority, dull, pitch),
            Command::Bed {
                bed,
                level,
                pan,
                dull,
            } => {
                if let Some(Some(layer)) = self.beds.get_mut(usize::from(bed)) {
                    layer.target = (
                        f32::from(level) / 255.0,
                        f32::from(pan) / 127.0,
                        f32::from(dull) / 255.0,
                    );
                }
            }
            Command::MusicIntensity(v) => self.intensity_target = f32::from(v) / 255.0,
            Command::MusicVolume(v) => self.set_music_volume(f32::from(v) / 255.0),
            Command::MasterVolume(v) => self.master = f32::from(v) / 255.0,
            Command::StopEffects => {
                for v in &mut self.voices {
                    v.active = false;
                }
                for bed in self.beds.iter_mut().flatten() {
                    bed.target.0 = 0.0;
                    bed.level = 0.0;
                }
            }
        }
    }

    fn set_music_volume(&mut self, volume: f32) {
        self.music_volume = volume;
        if volume <= 0.0 {
            self.music = None;
        } else if self.music.is_none() {
            let layer = |id: SoundId| {
                self.bank.clip(id).map(|clip| MusicLayer {
                    clip: *clip,
                    cursor: Cursor::start(clip),
                    step: clip.rate as f32 / self.rate as f32,
                })
            };
            if let (Some(calm), Some(battle)) =
                (layer(SoundId::MusicCalm), layer(SoundId::MusicBattle))
            {
                self.music = Some([calm, battle]);
            }
        }
    }

    fn play(&mut self, sound: SoundId, gain: u8, pan: i8, priority: u8, dull: u8, pitch: u8) {
        if sound.is_music() || sound.is_bed() {
            return;
        }
        let Some(clip) = self.bank.clip(sound) else {
            return;
        };
        let score = u32::from(priority) << 8 | u32::from(gain);
        let same = self
            .voices
            .iter()
            .filter(|v| v.active && v.sound == sound)
            .count();
        let slot = if same >= MAX_PER_SOUND {
            // Replace the oldest of its own kind, unless that one matters more.
            self.voices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.active && v.sound == sound)
                .min_by_key(|(_, v)| v.age)
                .filter(|(_, oldest)| score >= oldest.score)
                .map(|(i, _)| i)
        } else if let Some(free) = self.voices.iter().position(|v| !v.active) {
            Some(free)
        } else {
            // All busy: take the quietest, least important voice if this beats it.
            let (i, weakest) = self
                .voices
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| (v.score, v.age))
                .expect("at least one voice");
            (score > weakest.score).then_some(i)
        };
        let Some(i) = slot else {
            self.meter.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let level = f32::from(gain) / 255.0 * EFFECT_LEVEL;
        let (left, right) = pan_gains(f32::from(pan) / 127.0);
        self.age = self.age.wrapping_add(1);
        self.voices[i] = Voice {
            sound,
            cursor: Cursor::start(clip),
            step: clip.rate as f32 / self.rate as f32 * pitch_ratio(pitch),
            left: level * left,
            right: level * right,
            score,
            age: self.age,
            active: true,
            alpha: dull_alpha(f32::from(dull) / 255.0, self.rate),
            lp: 0.0,
        };
    }

    /// Fill `out` (interleaved stereo) with the next stretch of sound.
    pub fn render(&mut self, out: &mut [f32]) {
        while let Some(command) = self.commands.pop() {
            self.handle(command);
        }
        out.fill(0.0);
        let frames = out.len() / 2;

        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            let Some(clip) = self.bank.clip(v.sound) else {
                v.active = false;
                continue;
            };
            for frame in out.chunks_exact_mut(2) {
                match v.cursor.sample(clip, v.step) {
                    Some(s) => {
                        let s = if v.alpha < 1.0 {
                            v.lp += v.alpha * (s - v.lp);
                            v.lp
                        } else {
                            s
                        };
                        frame[0] += s * v.left;
                        frame[1] += s * v.right;
                    }
                    None => {
                        v.active = false;
                        break;
                    }
                }
            }
        }

        let seconds = frames as f32 / self.rate as f32;
        let glide = (seconds * BED_RATE).min(1.0);
        for bed in self.beds.iter_mut().flatten() {
            if bed.silent() {
                // Out of earshot: the next fade-in starts where it is told,
                // not from wherever it faded out.
                bed.level = 0.0;
                (bed.pan, bed.dull) = (bed.target.1, bed.target.2);
                bed.lp = 0.0;
                continue;
            }
            let (level, pan, dull) = bed.target;
            let from = bed.level;
            bed.level += (level - bed.level) * glide;
            bed.pan += (pan - bed.pan) * glide;
            bed.dull += (dull - bed.dull) * glide;
            let alpha = dull_alpha(bed.dull, self.rate);
            let pan = bed.pan;
            let (left, right) = pan_gains(pan);
            // The level ramps across the block, so a change never clicks.
            let slope = (bed.level - from) / frames as f32;
            for (i, frame) in out.chunks_exact_mut(2).enumerate() {
                // Looping clips never end.
                let s = bed.cursor.sample(&bed.clip, bed.step).unwrap_or(0.0);
                bed.lp += alpha * (s - bed.lp);
                let s = bed.lp * (from + slope * i as f32) * BED_LEVEL;
                frame[0] += s * left;
                frame[1] += s * right;
            }
        }

        if let Some(layers) = &mut self.music {
            let blend = (seconds * INTENSITY_RATE).min(1.0);
            self.intensity += (self.intensity_target - self.intensity) * blend;
            let level = self.music_volume * MUSIC_LEVEL;
            // The calm layer thins out as the battle layer comes in.
            let gains = [level * (1.0 - 0.6 * self.intensity), level * self.intensity];
            for (layer, gain) in layers.iter_mut().zip(gains) {
                for frame in out.chunks_exact_mut(2) {
                    // Looping clips never end.
                    let s = layer.cursor.sample(&layer.clip, layer.step).unwrap_or(0.0) * gain;
                    frame[0] += s;
                    frame[1] += s;
                }
            }
        }

        for s in out.iter_mut() {
            *s = soft_clip(*s * self.master);
        }
        let active = self.voices.iter().filter(|v| v.active).count();
        self.meter.active.store(active as u32, Ordering::Relaxed);
    }
}

/// Linear for quiet sounds, rounding off smoothly to ±1 instead of clipping.
fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-1.5, 1.5);
    x - x * x * x * (4.0 / 27.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{ClipSource, write_bank};
    use crate::queue::{Producer, channel};

    const RATE: u32 = 8000;

    fn tone(n: usize, amp: f32, freq: f32) -> Vec<i16> {
        (0..n)
            .map(|i| ((i as f32 * freq).sin() * amp * 20000.0) as i16)
            .collect()
    }

    fn rig(clip_len: usize) -> (Mixer, Producer, Arc<Meter>) {
        let pcm = tone(clip_len, 1.0, 0.2);
        let calm = tone(4000, 1.0, 0.05);
        let battle = tone(4000, 1.0, 0.4);
        let mut sources: Vec<ClipSource> = [
            SoundId::ArrowRelease,
            SoundId::ArrowImpact,
            SoundId::Clash,
            SoundId::Charge,
        ]
        .into_iter()
        .map(|id| ClipSource {
            id,
            rate: RATE,
            looped: false,
            pcm: &pcm,
        })
        .collect();
        sources.push(ClipSource {
            id: SoundId::MusicCalm,
            rate: RATE,
            looped: true,
            pcm: &calm,
        });
        sources.push(ClipSource {
            id: SoundId::MusicBattle,
            rate: RATE,
            looped: true,
            pcm: &battle,
        });
        let blob: &'static [u8] = Box::leak(write_bank(&sources).into_boxed_slice());
        let (tx, rx) = channel(256);
        let meter = Arc::new(Meter::default());
        let mixer = Mixer::new(Bank::parse(blob).unwrap(), RATE, rx, meter.clone());
        (mixer, tx, meter)
    }

    fn play(sound: SoundId, gain: u8, pan: i8, priority: u8) -> Command {
        Command::Play {
            sound,
            gain,
            pan,
            priority,
            dull: 0,
            pitch: 128,
        }
    }

    fn muffled(sound: SoundId, dull: u8) -> Command {
        Command::Play {
            sound,
            gain: 255,
            pan: 0,
            priority: 100,
            dull,
            pitch: 128,
        }
    }

    /// A rig whose bank also holds a looping bed of a fast tone (so a
    /// muffled bed is easy to tell from a clear one).
    fn bed_rig() -> (Mixer, Producer) {
        let pcm = tone(2000, 1.0, 1.5);
        let sources: Vec<ClipSource> = SoundId::BEDS
            .into_iter()
            .map(|id| ClipSource {
                id,
                rate: RATE,
                looped: true,
                pcm: &pcm,
            })
            .collect();
        let blob: &'static [u8] = Box::leak(write_bank(&sources).into_boxed_slice());
        let (tx, rx) = channel(64);
        let mixer = Mixer::new(
            Bank::parse(blob).unwrap(),
            RATE,
            rx,
            Arc::new(Meter::default()),
        );
        (mixer, tx)
    }

    fn bed(bed: u8, level: u8, pan: i8, dull: u8) -> Command {
        Command::Bed {
            bed,
            level,
            pan,
            dull,
        }
    }

    fn render(mixer: &mut Mixer, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        mixer.render(&mut out);
        out
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    #[test]
    fn silent_until_something_plays() {
        let (mut mixer, _tx, meter) = rig(1000);
        assert_eq!(peak(&render(&mut mixer, 512)), 0.0);
        assert_eq!(meter.active.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_sound_plays_then_ends() {
        let (mut mixer, tx, meter) = rig(1000);
        tx.push(play(SoundId::Clash, 255, 0, 100));
        assert!(peak(&render(&mut mixer, 500)) > 0.05);
        assert_eq!(meter.active.load(Ordering::Relaxed), 1);
        render(&mut mixer, 700);
        assert_eq!(meter.active.load(Ordering::Relaxed), 0);
        assert_eq!(peak(&render(&mut mixer, 100)), 0.0);
    }

    #[test]
    fn resamples_to_the_device_rate() {
        // A 1000-sample clip at 8 kHz lasts 2000 frames on a 16 kHz device.
        let (mut mixer, tx, meter) = rig(1000);
        mixer.rate = 16000;
        tx.push(play(SoundId::Clash, 255, 0, 100));
        render(&mut mixer, 1900);
        assert_eq!(meter.active.load(Ordering::Relaxed), 1);
        render(&mut mixer, 200);
        assert_eq!(meter.active.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn pan_favours_one_side() {
        let (mut mixer, tx, _) = rig(1000);
        tx.push(play(SoundId::Clash, 255, 127, 100));
        let out = render(&mut mixer, 500);
        let left = peak(&out.iter().step_by(2).copied().collect::<Vec<_>>());
        let right = peak(&out.iter().skip(1).step_by(2).copied().collect::<Vec<_>>());
        assert_eq!(left, 0.0);
        assert!(right > 0.05);
    }

    #[test]
    fn gain_scales_volume() {
        let (mut loud, tx, _) = rig(1000);
        tx.push(play(SoundId::Clash, 255, 0, 100));
        let (mut quiet, tx2, _) = rig(1000);
        tx2.push(play(SoundId::Clash, 64, 0, 100));
        assert!(peak(&render(&mut loud, 500)) > 3.0 * peak(&render(&mut quiet, 500)));
    }

    #[test]
    fn one_sound_cannot_take_every_voice() {
        let (mut mixer, tx, meter) = rig(4000);
        for _ in 0..20 {
            tx.push(play(SoundId::Clash, 200, 0, 100));
        }
        render(&mut mixer, 64);
        assert_eq!(meter.active.load(Ordering::Relaxed), MAX_PER_SOUND as u32);
    }

    #[test]
    fn full_mixer_drops_quiet_sounds_and_keeps_loud_ones() {
        let (mut mixer, tx, meter) = rig(4000);
        let kinds = [
            SoundId::ArrowRelease,
            SoundId::ArrowImpact,
            SoundId::Clash,
            SoundId::Charge,
        ];
        for i in 0..MAX_VOICES {
            tx.push(play(kinds[i % 4], 100, 0, 50));
        }
        render(&mut mixer, 64);
        assert_eq!(meter.active.load(Ordering::Relaxed), MAX_VOICES as u32);
        // A weaker sound finds no room.
        tx.push(play(SoundId::Clash, 10, 0, 10));
        render(&mut mixer, 64);
        assert_eq!(meter.dropped.load(Ordering::Relaxed), 1);
        // A more important one pushes a weak voice out.
        tx.push(play(SoundId::Clash, 255, 0, 250));
        render(&mut mixer, 64);
        assert_eq!(meter.dropped.load(Ordering::Relaxed), 1);
        assert_eq!(meter.active.load(Ordering::Relaxed), MAX_VOICES as u32);
    }

    #[test]
    fn stop_effects_silences_them() {
        let (mut mixer, tx, meter) = rig(4000);
        tx.push(play(SoundId::Clash, 255, 0, 100));
        render(&mut mixer, 64);
        tx.push(Command::StopEffects);
        assert_eq!(peak(&render(&mut mixer, 64)), 0.0);
        assert_eq!(meter.active.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn music_loops_and_follows_intensity() {
        let (mut mixer, tx, _) = rig(100);
        // Off by default.
        assert_eq!(peak(&render(&mut mixer, 256)), 0.0);
        tx.push(Command::MusicVolume(255));
        let calm = peak(&render(&mut mixer, 6000)); // longer than the 4000-sample loop
        assert!(calm > 0.05, "calm music should play and keep looping");
        tx.push(Command::MusicIntensity(255));
        render(&mut mixer, 16000); // let the crossfade settle
        let tail = render(&mut mixer, 2000);
        assert!(peak(&tail) > 0.05);
        assert!(mixer.intensity > 0.95);
        tx.push(Command::MusicVolume(0));
        assert_eq!(peak(&render(&mut mixer, 256)), 0.0);
    }

    #[test]
    fn distance_muffles_a_sound() {
        // A 2.5 kHz tone: well above the muffled cutoff.
        let pcm = tone(2000, 1.0, 2.0);
        let blob: &'static [u8] = Box::leak(
            write_bank(&[ClipSource {
                id: SoundId::Clash,
                rate: RATE,
                looped: false,
                pcm: &pcm,
            }])
            .into_boxed_slice(),
        );
        let heard = |dull| {
            let (tx, rx) = channel(16);
            let mut mixer = Mixer::new(
                Bank::parse(blob).unwrap(),
                RATE,
                rx,
                Arc::new(Meter::default()),
            );
            tx.push(muffled(SoundId::Clash, dull));
            peak(&render(&mut mixer, 500))
        };
        let (clear, far) = (heard(0), heard(255));
        assert!(clear > 0.05);
        assert!(far < clear * 0.5, "far {far} against clear {clear}");
    }

    #[test]
    fn pitch_changes_how_long_a_sound_lasts() {
        assert_eq!(pitch_ratio(128), 1.0);
        assert!((pitch_ratio(0) - 0.5).abs() < 1e-6);
        let ended_after = |pitch: u8| {
            let (mut mixer, tx, meter) = rig(1000);
            tx.push(Command::Play {
                sound: SoundId::Clash,
                gain: 255,
                pan: 0,
                priority: 100,
                dull: 0,
                pitch,
            });
            let mut frames = 0;
            while frames == 0 || meter.active.load(Ordering::Relaxed) > 0 {
                render(&mut mixer, 50);
                frames += 50;
            }
            frames
        };
        assert!(ended_after(0) > 2 * ended_after(128) - 150);
        assert!(ended_after(255) < ended_after(128) / 2 + 100);
    }

    #[test]
    fn dull_alpha_runs_from_clear_to_muffled() {
        assert_eq!(dull_alpha(0.0, 48_000), 1.0);
        let (mid, far) = (dull_alpha(0.5, 48_000), dull_alpha(1.0, 48_000));
        assert!(1.0 > mid && mid > far && far > 0.0);
    }

    #[test]
    fn a_bed_is_silent_until_told_then_glides_in_and_out() {
        let (mut mixer, tx) = bed_rig();
        assert_eq!(peak(&render(&mut mixer, 512)), 0.0);
        tx.push(bed(1, 255, 0, 0));
        let first = peak(&render(&mut mixer, 64));
        for _ in 0..40 {
            render(&mut mixer, 512);
        }
        let settled = peak(&render(&mut mixer, 512));
        assert!(first < settled * 0.5, "it should fade in, not jump");
        assert!(settled > 0.1);
        tx.push(bed(1, 0, 0, 0));
        for _ in 0..80 {
            render(&mut mixer, 512);
        }
        assert!(peak(&render(&mut mixer, 512)) < 0.01);
    }

    #[test]
    fn a_bed_pans_and_dulls() {
        let (mut mixer, tx) = bed_rig();
        tx.push(bed(3, 255, 127, 255));
        for _ in 0..60 {
            render(&mut mixer, 512);
        }
        let out = render(&mut mixer, 512);
        let side = |skip| {
            peak(
                &out.iter()
                    .skip(skip)
                    .step_by(2)
                    .copied()
                    .collect::<Vec<_>>(),
            )
        };
        assert!(side(0) < 0.02, "left should be empty");
        let muffled = side(1);
        let (mut clear, tx2) = bed_rig();
        tx2.push(bed(3, 255, 127, 0));
        for _ in 0..60 {
            render(&mut clear, 512);
        }
        let open = peak(&render(&mut clear, 512));
        assert!(muffled < open * 0.5, "{muffled} against {open}");
    }

    #[test]
    fn beds_are_not_one_shots_and_stop_effects_silences_them() {
        let (mut mixer, tx) = bed_rig();
        tx.push(muffled(SoundId::BedWind, 0));
        assert_eq!(peak(&render(&mut mixer, 512)), 0.0);
        tx.push(bed(0, 255, 0, 0));
        for _ in 0..40 {
            render(&mut mixer, 512);
        }
        tx.push(Command::StopEffects);
        assert_eq!(peak(&render(&mut mixer, 512)), 0.0);
    }

    #[test]
    fn master_volume_mutes() {
        let (mut mixer, tx, _) = rig(1000);
        tx.push(Command::MasterVolume(0));
        tx.push(play(SoundId::Clash, 255, 0, 100));
        assert_eq!(peak(&render(&mut mixer, 500)), 0.0);
    }

    #[test]
    fn soft_clip_is_gentle() {
        assert!((soft_clip(0.1) - 0.1).abs() < 0.001);
        assert!(soft_clip(1.5) <= 1.0001);
        assert!(soft_clip(10.0) <= 1.0001);
        assert!(soft_clip(-10.0) >= -1.0001);
    }
}
