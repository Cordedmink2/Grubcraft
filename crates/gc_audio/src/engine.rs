//! Output to the sound card (`cpal`) and the handle the game talks to.

use crate::bank::{Bank, BankError, SoundId};
use crate::mixer::{Meter, Mixer};
use crate::queue::{Command, Producer, channel};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Frames mixed per pass inside the audio callback.
const CHUNK_FRAMES: usize = 1024;
const QUEUE_CAPACITY: usize = 512;

#[derive(Debug)]
pub enum AudioError {
    Bank(BankError),
    NoDevice,
    Device(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::Bank(e) => write!(f, "{e}"),
            AudioError::NoDevice => write!(f, "no audio output device"),
            AudioError::Device(e) => write!(f, "audio device: {e}"),
        }
    }
}

impl std::error::Error for AudioError {}

impl From<BankError> for AudioError {
    fn from(e: BankError) -> Self {
        AudioError::Bank(e)
    }
}

fn device_err(e: impl fmt::Display) -> AudioError {
    AudioError::Device(e.to_string())
}

/// A running audio output. Dropping it stops the sound.
pub struct Audio {
    commands: Producer,
    meter: Arc<Meter>,
    /// Set by the stream's error callback (headphones unplugged, device
    /// removed): the stream is dead and the game should call `start` again.
    lost: Arc<AtomicBool>,
    device: String,
    rate: u32,
    _stream: Stream,
}

impl Audio {
    /// Open the default output device and start mixing. The bank must hold
    /// every sound; the game carries on silently if this returns an error.
    pub fn start(bank: Bank<'static>) -> Result<Audio, AudioError> {
        bank.require_all()?;
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
        let supported = device.default_output_config().map_err(device_err)?;
        let rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();

        let (producer, consumer) = channel(QUEUE_CAPACITY);
        let meter = Arc::new(Meter::default());
        let mut mixer = Mixer::new(bank, rate, consumer, meter.clone());
        let mut scratch = vec![0.0f32; CHUNK_FRAMES * 2];
        let lost = Arc::new(AtomicBool::new(false));
        let on_error = {
            let lost = lost.clone();
            // The error text goes nowhere useful from the audio thread; the
            // game logs that the stream was lost and reopens it.
            move |_: cpal::StreamError| lost.store(true, Ordering::Release)
        };

        let stream = match format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    fill(&mut mixer, &mut scratch, data, channels, |s| s);
                },
                on_error.clone(),
                None,
            ),
            SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _| {
                    fill(&mut mixer, &mut scratch, data, channels, |s| {
                        (s * 32767.0) as i16
                    });
                },
                on_error,
                None,
            ),
            other => {
                return Err(AudioError::Device(format!(
                    "unsupported sample format {other:?}"
                )));
            }
        }
        .map_err(device_err)?;
        stream.play().map_err(device_err)?;

        Ok(Audio {
            commands: producer,
            meter,
            lost,
            device: device.name().unwrap_or_else(|_| "unknown".into()),
            rate,
            _stream: stream,
        })
    }

    /// Play a sound. `gain` is 0..=1 (already reduced for distance), `pan` is
    /// -1 (left) to 1 (right), `priority` decides who is cut off when the 32
    /// voices are all busy. Silent sounds are not sent.
    pub fn play(&self, sound: SoundId, gain: f32, pan: f32, priority: u8) {
        self.play_muffled(sound, gain, pan, priority, 0.0, 1.0);
    }

    /// Like [`play`](Audio::play), with the highs taken off: `dull` is 0
    /// (clear) to 1 (muffled), how distance changes a sound; `speed` plays
    /// it faster (above 1) or slower, up to an octave either way.
    pub fn play_muffled(
        &self,
        sound: SoundId,
        gain: f32,
        pan: f32,
        priority: u8,
        dull: f32,
        speed: f32,
    ) {
        let gain = to_byte(gain);
        if gain == 0 {
            return;
        }
        self.commands.push(Command::Play {
            sound,
            gain,
            pan: to_pan(pan),
            priority,
            dull: to_byte(dull),
            pitch: (128.0 + 128.0 * speed.clamp(0.5, 1.99).log2())
                .round()
                .clamp(0.0, 255.0) as u8,
        });
    }

    /// Set an ambient bed (`SoundId::BEDS[bed]`): `level` 0..=1 (0 fades it
    /// out), `pan` -1..=1 and `dull` 0..=1 as for [`play_muffled`](Audio::play_muffled).
    /// The mixer glides to the new values, so this is cheap to call as the
    /// scene changes.
    pub fn set_bed(&self, bed: usize, level: f32, pan: f32, dull: f32) {
        self.commands.push(Command::Bed {
            bed: bed as u8,
            level: to_byte(level),
            pan: to_pan(pan),
            dull: to_byte(dull),
        });
    }

    /// 0 = calm music, 1 = full battle layer; it fades between them.
    pub fn set_music_intensity(&self, intensity: f32) {
        self.commands
            .push(Command::MusicIntensity(to_byte(intensity)));
    }

    /// 0 stops the music.
    pub fn set_music_volume(&self, volume: f32) {
        self.commands.push(Command::MusicVolume(to_byte(volume)));
    }

    pub fn set_master_volume(&self, volume: f32) {
        self.commands.push(Command::MasterVolume(to_byte(volume)));
    }

    pub fn stop_effects(&self) {
        self.commands.push(Command::StopEffects);
    }

    /// Effect voices playing now.
    pub fn voices_active(&self) -> u32 {
        self.meter.active.load(Ordering::Relaxed)
    }

    /// Sounds dropped so far because every voice was busy with something louder.
    pub fn sounds_dropped(&self) -> u32 {
        self.meter.dropped.load(Ordering::Relaxed)
    }

    /// True once the output stream has died. This `Audio` is silent for good;
    /// drop it and `start` a new one.
    pub fn stream_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    pub fn device_name(&self) -> &str {
        &self.device
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }
}

fn to_pan(v: f32) -> i8 {
    (v.clamp(-1.0, 1.0) * 127.0).round() as i8
}

fn to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Mix stereo in chunks and spread it over the device's channels.
fn fill<T: Copy>(
    mixer: &mut Mixer,
    scratch: &mut [f32],
    data: &mut [T],
    channels: usize,
    convert: impl Fn(f32) -> T,
) {
    for out in data.chunks_mut(CHUNK_FRAMES * channels) {
        let frames = out.len() / channels;
        let stereo = &mut scratch[..frames * 2];
        mixer.render(stereo);
        for (frame, pair) in out.chunks_exact_mut(channels).zip(stereo.chunks_exact(2)) {
            match channels {
                1 => frame[0] = convert((pair[0] + pair[1]) * 0.5),
                _ => {
                    frame[0] = convert(pair[0]);
                    frame[1] = convert(pair[1]);
                    for extra in &mut frame[2..] {
                        *extra = convert(0.0);
                    }
                }
            }
        }
    }
}
