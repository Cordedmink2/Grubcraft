//! Turns what happens in the match into sound.
//!
//! The simulation knows nothing about audio. After each tick this module
//! reads the same public views the renderer reads (regiments and volleys in
//! flight), compares them with the previous tick, and plays a sound for each
//! change: volleys leaving and landing, blows in melee, a regiment wavering,
//! breaking, shattering or rallying. Sounds are placed relative to the
//! camera's ground point: quieter with distance (less so when zoomed out)
//! and panned left or right.

use crate::camera::RtsCamera;
use crate::from_fx_vec;
use gc_audio::{Audio, Bank, SoundId};
use gc_sim::{MoraleState, RegimentHandle, World};
use glam::Vec2;

/// The sound bank written by `cargo run -p gc_sfx`.
static BANK: &[u8] = include_bytes!("../../../assets/audio/sounds.gcsnd");

/// How far the camera "hears" at the nearest zoom, and extra per unit of
/// camera distance, in tiles.
const HEAR_BASE: f32 = 50.0;
const HEAR_PER_ZOOM: f32 = 0.8;
/// Music volume while playing.
const MUSIC_VOLUME: f32 = 0.6;
/// Engaged regiments near the camera that bring the battle music to full.
const FULL_BATTLE: f32 = 4.0;
/// Ticks between blows heard from one regiment.
const BLOW_COOLDOWN: u32 = 3;
/// Every this many ticks an engaged regiment is heard even without a death.
const AMBIENT_EVERY: u32 = 10;

// Who is cut off first when all 32 voices are busy.
const PRI_ARROWS: u8 = 70;
const PRI_BLOWS: u8 = 100;
const PRI_CHARGE: u8 = 170;
const PRI_MORALE: u8 = 220;
const PRI_UI: u8 = 255;

/// Short clicks for the interface.
#[derive(Clone, Copy, Debug)]
pub enum UiSound {
    Select,
    Move,
    Attack,
}

#[derive(Clone, Copy)]
struct Seen {
    handle: RegimentHandle,
    morale: MoraleState,
    models: u16,
    engaged: bool,
    /// Tick before which no further blow is played for it.
    quiet_until: u32,
}

/// A volley in the air, remembered so its landing can be heard.
#[derive(Clone, Copy)]
struct Flight {
    key: (u32, u32, i32, i32),
    aim: Vec2,
    shots: u16,
    in_view: bool,
}

pub struct Sounds {
    audio: Audio,
    seen: Vec<Option<Seen>>,
    flights: Vec<Flight>,
    focus: Vec2,
    right: Vec2,
    hearing: f32,
    last_intensity: u8,
    muted: bool,
    music_on: bool,
}

impl Sounds {
    /// Open the sound card and start the music. None (with a note on stderr)
    /// if there is no usable output; the game then runs silently.
    pub fn start(regiment_capacity: usize) -> Option<Sounds> {
        let started = Bank::parse(BANK)
            .map_err(gc_audio::AudioError::from)
            .and_then(Audio::start);
        match started {
            Ok(audio) => {
                eprintln!(
                    "audio: {} at {} Hz",
                    audio.device_name(),
                    audio.sample_rate()
                );
                audio.set_music_volume(MUSIC_VOLUME);
                Some(Sounds {
                    audio,
                    seen: vec![None; regiment_capacity],
                    flights: Vec::with_capacity(256),
                    focus: Vec2::ZERO,
                    right: Vec2::X,
                    hearing: HEAR_BASE,
                    last_intensity: 0,
                    muted: false,
                    music_on: true,
                })
            }
            Err(e) => {
                eprintln!("audio off: {e}");
                None
            }
        }
    }

    /// Call after every simulation tick.
    pub fn observe(&mut self, world: &World, camera: &RtsCamera) {
        self.focus = camera.focus;
        self.right = Vec2::new(camera.yaw.cos(), -camera.yaw.sin());
        self.hearing = HEAR_BASE + camera.distance * HEAR_PER_ZOOM;
        let tick = world.tick_count();

        self.volleys(world);

        let mut fighting = 0.0f32;
        for r in world.regiments.iter() {
            let i = r.handle.index as usize;
            let at = from_fx_vec(r.position);
            let prev = self.seen[i].filter(|s| s.handle == r.handle);
            let mut quiet_until = prev.map_or(0, |s| s.quiet_until);
            if let Some(p) = prev {
                if p.morale != r.morale {
                    self.morale_change(p.morale, r.morale, at);
                }
                if r.engaged && !p.engaged {
                    self.play_at(SoundId::Charge, at, 1.0, 1.4, PRI_CHARGE);
                }
                if r.engaged && tick >= quiet_until {
                    if r.models < p.models {
                        let losses = f32::from(p.models - r.models);
                        let gain = 0.55 + 0.15 * losses.min(3.0);
                        self.play_at(SoundId::Clash, at, gain, 1.0, PRI_BLOWS);
                        quiet_until = tick + BLOW_COOLDOWN;
                    } else if (tick + i as u32 * 7).is_multiple_of(AMBIENT_EVERY) {
                        self.play_at(SoundId::Clash, at, 0.4, 1.0, PRI_BLOWS);
                    }
                }
            }
            self.seen[i] = Some(Seen {
                handle: r.handle,
                morale: r.morale,
                models: r.models,
                engaged: r.engaged,
                quiet_until,
            });
            if r.engaged && (at - self.focus).length() < self.hearing * 1.5 {
                fighting += 1.0;
            }
        }

        let intensity = (fighting / FULL_BATTLE).min(1.0);
        let byte = (intensity * 20.0).round() as u8;
        if byte != self.last_intensity {
            self.last_intensity = byte;
            self.audio.set_music_intensity(intensity);
        }
    }

    /// Volleys launched this tick sound a twang; volleys that were in flight
    /// last tick and are gone now have landed.
    fn volleys(&mut self, world: &World) {
        for f in &mut self.flights {
            f.in_view = false;
        }
        let mut launches = Vec::new();
        for v in world.volleys() {
            let key = (v.launch_tick, v.impact_tick, v.from.x.raw(), v.from.y.raw());
            match self.flights.iter_mut().find(|f| f.key == key) {
                Some(f) => f.in_view = true,
                None => launches.push((from_fx_vec(v.from), key, from_fx_vec(v.aim), v.shots)),
            }
        }
        let landed: Vec<Flight> = self
            .flights
            .iter()
            .filter(|f| !f.in_view)
            .copied()
            .collect();
        self.flights.retain(|f| f.in_view);
        for (from, key, aim, shots) in launches {
            self.play_at(SoundId::ArrowRelease, from, 0.8, 1.0, PRI_ARROWS);
            self.flights.push(Flight {
                key,
                aim,
                shots,
                in_view: true,
            });
        }
        for f in landed {
            let gain = 0.5 + 0.5 * (f32::from(f.shots) / 16.0).min(1.0);
            self.play_at(SoundId::ArrowImpact, f.aim, gain, 1.0, PRI_ARROWS);
        }
    }

    fn morale_change(&self, from: MoraleState, to: MoraleState, at: Vec2) {
        match (from, to) {
            (_, MoraleState::Wavering) if !from.broken() => {
                self.play_at(SoundId::Waver, at, 0.8, 1.2, PRI_MORALE);
            }
            (_, MoraleState::Routing) => self.play_at(SoundId::RoutHorn, at, 1.0, 1.8, PRI_MORALE),
            (_, MoraleState::Shattered) => self.play_at(SoundId::Shatter, at, 1.0, 2.0, PRI_MORALE),
            (MoraleState::Routing, _) => self.play_at(SoundId::Rally, at, 1.0, 1.6, PRI_MORALE),
            _ => {}
        }
    }

    /// Play a sound at a map position: quieter with distance from the
    /// camera's ground point, panned by which side of the screen it is on.
    /// `reach` stretches how far it carries (horns carry further than blows).
    fn play_at(&self, sound: SoundId, at: Vec2, gain: f32, reach: f32, priority: u8) {
        let radius = self.hearing * reach;
        let offset = at - self.focus;
        let distance = offset.length();
        if distance >= radius {
            return;
        }
        let near = 1.0 - distance / radius;
        let pan = offset.dot(self.right) / (radius * 0.5);
        self.audio.play(sound, gain * near * near, pan, priority);
    }

    /// An ambush sprung on or by the side you command: a war-drum roll and
    /// a horn, loud wherever the camera is, panned towards it.
    pub fn ambush(&self, at: Vec2) {
        let pan = ((at - self.focus).dot(self.right) / self.hearing.max(1.0)).clamp(-1.0, 1.0);
        self.audio.play(SoundId::Charge, 1.0, pan, PRI_MORALE);
        self.audio.play(SoundId::RoutHorn, 0.6, pan, PRI_MORALE);
    }

    /// Interface click: full volume, centred.
    pub fn ui(&self, sound: UiSound) {
        let id = match sound {
            UiSound::Select => SoundId::UiSelect,
            UiSound::Move => SoundId::UiMove,
            UiSound::Attack => SoundId::UiAttack,
        };
        self.audio.play(id, 0.8, 0.0, PRI_UI);
    }

    /// M: silence or restore everything.
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.audio
            .set_master_volume(if self.muted { 0.0 } else { 1.0 });
    }

    /// N: music off or on, effects stay.
    pub fn toggle_music(&mut self) {
        self.music_on = !self.music_on;
        self.audio
            .set_music_volume(if self.music_on { MUSIC_VOLUME } else { 0.0 });
    }

    /// One line for the dev overlay.
    pub fn overlay_line(&self) -> String {
        format!(
            "{} {} Hz   voices {}/{}   dropped {}   {}{}",
            self.audio.device_name(),
            self.audio.sample_rate(),
            self.audio.voices_active(),
            gc_audio::MAX_VOICES,
            self.audio.sounds_dropped(),
            if self.muted { "MUTED  " } else { "" },
            if self.music_on {
                "music on"
            } else {
                "music off"
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_sound_bank_is_complete() {
        let bank = Bank::parse(BANK).expect("embedded sound bank parses");
        bank.require_all().expect("every sound is present");
    }
}
